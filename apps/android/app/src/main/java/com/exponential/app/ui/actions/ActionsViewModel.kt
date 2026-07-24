package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionsApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

// The Actions surface (EXP-253, mobile = view + run only): the selected
// team's action prompts over tRPC (`actions.list` — deliberately NOT an
// Electric shape) plus the remote-run flow. After the server accepts a start,
// the model watches the synced coding_sessions DAO flow for the row the
// desktop inserts (this action's id + the caller's own userId + a recent
// startedAt) and surfaces its id exactly once so the screen can jump into the
// existing agent session viewer.

data class ActionsState(
    val actions: List<ActionDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

/** Run feedback: an informational Sent caption vs a persistent red Failed. */
sealed interface ActionRunState {
    data object Idle : ActionRunState
    data object Sending : ActionRunState
    data class Sent(val deviceLabel: String) : ActionRunState
    data class Failed(val message: String) : ActionRunState
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val actionsApi: ActionsApi,
    private val steerApi: SteerApi,
    selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's online desktops (relay presence). null = not loaded yet.
    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    val devices: StateFlow<List<SteerDevice>?> = _devices

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    // The freshly-started run's coding session id — consumed exactly once by
    // the screen's navigation (consumeStartedSession).
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    private var watchJob: Job? = null

    val state: StateFlow<ActionsState> =
        combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
            accountId to teamId
        }.flatMapLatest { (accountId, teamId) ->
            flow {
                if (accountId == null || teamId == null) {
                    emit(ActionsState(loading = false))
                    return@flow
                }
                emit(ActionsState(loading = true))
                emit(
                    runCatching { actionsApi.list(accountId, teamId) }.fold(
                        onSuccess = { ActionsState(actions = it, loading = false) },
                        onFailure = {
                            if (it is CancellationException) throw it
                            ActionsState(
                                loading = false,
                                error = trpcErrorMessage(it, "Couldn't load actions"),
                            )
                        },
                    ),
                )
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionsState())

    init {
        // Steer availability + device presence, re-fetched on account switch
        // (the AgentsViewModel pattern).
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _devices.value = null
                _runState.value = ActionRunState.Idle
                if (accountId == null) {
                    _steerEnabled.value = false
                    _devices.value = emptyList()
                    return@collectLatest
                }
                val enabled = runCatching { steerApi.config(accountId).enabled }
                    .getOrDefault(false)
                _steerEnabled.value = enabled
                _devices.value = if (enabled) {
                    runCatching { steerApi.myDevices(accountId).devices }.getOrDefault(emptyList())
                } else {
                    emptyList()
                }
            }
        }
    }

    /** Re-poll device presence (on screen resume) — no-op until steer resolves on. */
    fun refreshDevices() {
        if (_steerEnabled.value != true) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { steerApi.myDevices(accountId).devices }
                .onSuccess { _devices.value = it }
        }
    }

    fun consumeStartedSession() {
        _startedSessionId.value = null
    }

    /**
     * Remote-run [action] on [device] (Claude-only v1 — model/effort are the
     * only options; null = desktop settings default), then watch the synced
     * coding_sessions flow for the desktop's row. Sent state re-enables after
     * a grace window in case the desktop never picks up.
     */
    fun runAction(action: ActionDto, device: SteerDevice, model: String?, effort: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _runState.value = ActionRunState.Sending
            try {
                steerApi.startActionSession(accountId, action.id, device.deviceId, model, effort)
                _runState.value = ActionRunState.Sent(device.deviceLabel.ifBlank { device.deviceId })
                watchForStartedRun(action.id, auth.userId.value)
                delay(30_000)
                if (_runState.value is ActionRunState.Sent) {
                    _runState.value = ActionRunState.Idle
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    // Wait for the desktop-inserted session row of THIS start: matching
    // action, the caller's own userId, and a startedAt after the send (with
    // clock-skew slack) — an old run of the same action must never re-trigger
    // navigation. Gives up silently after a deadline.
    private fun watchForStartedRun(actionId: String, userId: String?) {
        watchJob?.cancel()
        if (userId == null) return
        val cutoffMs = System.currentTimeMillis() - 120_000
        watchJob = viewModelScope.launch {
            val match = withTimeoutOrNull(180_000) {
                dbFlow.scopedQuery(emptyList()) {
                    it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
                }.mapNotNull { sessions ->
                    sessions.firstOrNull { session ->
                        session.actionId == actionId &&
                            session.userId == userId &&
                            (CodingSessionLiveness.parseEpochMs(session.startedAt) ?: 0L) >= cutoffMs
                    }
                }.first()
            }
            if (match != null) {
                _runState.value = ActionRunState.Idle
                _startedSessionId.value = match.id
            }
        }
    }
}
