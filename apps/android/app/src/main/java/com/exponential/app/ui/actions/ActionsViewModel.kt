package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.builtinActions
import com.exponential.app.data.api.toActionDto
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.steer.ActionRunState
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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json

// The Actions surface (EXP-253, mobile = view + run only): the selected
// team's action prompts LIVE from the synced actions shape (EXP-268 — the
// local Room flow, body-less by design; the virtual "Fix merge conflicts"
// builtin is prepended client-side, while "Create action" hides behind the
// screen's "New action" button since EXP-431) plus the remote-run
// flow. After the server
// accepts a start,
// the model watches the synced coding_sessions DAO flow for the row the
// desktop inserts (this action's NAME + the caller's own userId + a recent
// startedAt — never the action id: the builtin "Create action" run's row
// carries action_id NULL, EXP-257) and surfaces its id exactly once so the
// screen can jump into the existing agent session viewer.

data class ActionsState(
    val actions: List<ActionDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The online machines a run can go to: the caller's own plus (EXP-432) the
    // selected team's shared servers, filtered to ONLINE so the flow keeps the
    // presence-only semantics the screen gates on. null = not loaded yet.
    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    val devices: StateFlow<List<SteerDevice>?> = _devices

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    // The freshly-started run's coding session id — consumed exactly once by
    // the screen's navigation (consumeStartedSession).
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    private var watchJob: Job? = null

    // The selected team, for the screen's "New action" entry point (EXP-431).
    val selectedTeamId: StateFlow<String?> = selection.selectedId

    // Live from the synced actions shape (EXP-268): the DAO orders by
    // sort_order then name; the fix-conflicts builtin is prepended (the
    // screens pin it first by the flag, never by sort order). "Create action"
    // is deliberately NOT listed (EXP-431) — creation lives behind the top
    // bar's "New action" button instead of posing as a runnable action.
    val state: StateFlow<ActionsState> =
        combine(dbFlow, selection.selectedId) { db, teamId ->
            db to teamId
        }.flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(ActionsState(loading = false))
            } else {
                db.actionDao().observeByTeam(teamId).map { rows ->
                    ActionsState(
                        actions = builtinActions(teamId)
                            .filter { it.id != DomainContract.builtinCreateActionId } +
                            rows.map { it.toActionDto(json) },
                        loading = false,
                    )
                }
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionsState())

    // Issues the unified sheet's Issues tab can queue (AgentsViewModel's
    // candidate rules): the selected team's repo-backed, live boards;
    // open issues, `updatedAt` desc.
    val startCandidates: StateFlow<List<StartIssueOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        selection.selectedId,
    ) { issues, boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            val eligibleBoards = boards
                .filter {
                    it.teamId == teamId &&
                        it.repositoryId != null &&
                        it.deletedAt == null
                }
                .associateBy { it.id }
            issues
                .filter {
                    it.boardId in eligibleBoards.keys &&
                        it.status !in TERMINAL_ISSUE_STATUSES &&
                        it.prState != DomainContract.prStateMerged
                }
                .sortedByDescending { it.updatedAt }
                .map { issue ->
                    StartIssueOption(
                        id = issue.id,
                        identifier = issue.identifier,
                        title = issue.title,
                        repositoryId = eligibleBoards[issue.boardId]?.repositoryId,
                        status = issue.status,
                        priority = issue.priority,
                    )
                }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // The account steer.config was last resolved for — it is env-derived and
    // static per INSTANCE, so a team switch must not re-run it (that would
    // blank `steerEnabled` and flicker the screen's run affordances).
    private var configuredAccountId: String? = null

    init {
        // Steer availability + device presence, re-fetched on account switch
        // (the AgentsViewModel pattern) and, since EXP-432, on team switch —
        // the shared machines the list carries belong to the SELECTED team.
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
                accountId to teamId
            }.collectLatest { (accountId, teamId) ->
                _devices.value = null
                _runState.value = ActionRunState.Idle
                if (accountId == null) {
                    configuredAccountId = null
                    _steerEnabled.value = false
                    _devices.value = emptyList()
                    return@collectLatest
                }
                if (configuredAccountId != accountId) {
                    _steerEnabled.value = null
                    _steerEnabled.value = runCatching { steerApi.config(accountId).enabled }
                        .getOrDefault(false)
                    configuredAccountId = accountId
                }
                _devices.value = if (_steerEnabled.value == true) {
                    fetchDevices(accountId, teamId) ?: emptyList()
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
            // A failed re-poll keeps the list the screen is already showing.
            fetchDevices(accountId, selection.selectedId.value)?.let { _devices.value = it }
        }
    }

    // The team-scoped registry (EXP-432) narrowed to what can take a run right
    // now: own machines plus teammates' shared servers, ONLINE only — the
    // screen treats an empty list as "no desktop online". null = lookup failed.
    private suspend fun fetchDevices(accountId: String, teamId: String?): List<SteerDevice>? =
        runCatching { devicesApi.list(accountId, teamId).devices.filter { it.online } }
            .getOrNull()

    fun consumeStartedSession() {
        _startedSessionId.value = null
    }

    /**
     * Remote-run [action] on [device] with the unified sheet's full [options]
     * + filled [inputs] (EXP-257 — same per-agent vocabulary as issue runs),
     * then watch the synced coding_sessions flow for the desktop's row. The
     * builtin "Create action" id additionally rides its teamId (the server
     * requires it there and forbids it otherwise). Sent state re-enables
     * after a grace window in case the desktop never picks up.
     */
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _runState.value = ActionRunState.Sending
            try {
                steerApi.startActionSession(
                    accountId,
                    actionId = action.id,
                    deviceId = device.deviceId,
                    options = options,
                    // Required for EVERY builtin (there is no DB row to derive
                    // the team from), forbidden otherwise — the server rejects
                    // both mistakes.
                    teamId = action.teamId.takeIf { action.isBuiltin },
                    inputs = inputs.takeIf { it.isNotEmpty() },
                )
                _runState.value = ActionRunState.Sent(device.deviceLabel.ifBlank { device.deviceId })
                watchForStartedRun(action.name, auth.userId.value)
                // Keep the Sent caption for the whole watch deadline (iOS
                // parity) — a slow desktop pickup can still navigate late,
                // and a captionless late jump reads as a glitch.
                delay(180_000)
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

    /**
     * Remote-start issues from the unified sheet's Issues tab (the
     * AgentsViewModel.startCoding twin, surfaced through the run captions):
     * 1 id launches a plain single session, 2+ a batch.
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        if (issueIds.isEmpty()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _runState.value = ActionRunState.Sending
            try {
                if (issueIds.size >= 2) {
                    steerApi.startSession(accountId, issueIds, device.deviceId, options)
                } else {
                    steerApi.startSession(accountId, issueIds.first(), device.deviceId, options)
                }
                _runState.value = ActionRunState.Sent(device.deviceLabel.ifBlank { device.deviceId })
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
    // action_name (builtin rows carry action_id NULL, so the name snapshot is
    // the only stable key — EXP-257), the caller's own userId, and a
    // startedAt after the send (with clock-skew slack) — an old run of the
    // same action must never re-trigger navigation. Gives up silently after a
    // deadline.
    private fun watchForStartedRun(actionName: String, userId: String?) {
        watchJob?.cancel()
        if (userId == null) return
        val cutoffMs = System.currentTimeMillis() - 120_000
        watchJob = viewModelScope.launch {
            val match = withTimeoutOrNull(180_000) {
                dbFlow.scopedQuery(emptyList()) {
                    it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
                }.mapNotNull { sessions ->
                    sessions.firstOrNull { session ->
                        session.actionName == actionName &&
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

// Terminal issue statuses ineligible to start a new coding run.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
