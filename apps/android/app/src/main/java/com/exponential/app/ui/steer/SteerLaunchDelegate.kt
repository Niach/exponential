package com.exponential.app.ui.steer

import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SessionsSendResult
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.ui.agent.IssueOption
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** Send feedback: an informational Sent caption vs a persistent red Failed. */
sealed interface ActionRunState {
    data object Idle : ActionRunState
    data object Sending : ActionRunState
    data class Sent(val deviceLabel: String) : ActionRunState
    data class Failed(val message: String) : ActionRunState
}

/**
 * EXP-1291 Threads v2: THE send helper — ONE `sessions.send` call for every
 * start and message (a new thread root, a new agent, a child, or a message
 * to an existing session). Stateless; the home composer, the thread
 * composer and the session view all go through it. Throws the tRPC error.
 */
class SessionSender @Inject constructor(
    private val auth: AuthRepository,
    private val sessionsApi: SessionsApi,
) {
    suspend fun send(input: SessionsSendInput): SessionsSendResult {
        val accountId = auth.activeAccountId.value ?: error("No active account")
        return sessionsApi.send(accountId, input)
    }
}

/**
 * Launcher plumbing for a composer host: steer availability, the team's
 * start devices, the issue candidate pool and the ONE send
 * ([send] → `sessions.send`). A new session is a `queued` row the device
 * picks up off the sessions shape, so the answer's `sessionId` lands in
 * [startedSessionId] at once (no post-send watch).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SteerLaunchDelegate @Inject constructor(
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    private val sender: SessionSender,
    holder: DatabaseHolder,
    selection: TeamSelection,
) {

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private var teamIdFlow: Flow<String?> = selection.selectedId

    private val _enabled = MutableStateFlow<Boolean?>(null)
    /** Steer availability on this instance. null = not resolved yet. */
    val enabled: StateFlow<Boolean?> = _enabled

    private val noDevices = MutableStateFlow<List<SteerDevice>?>(null)
    private var _devices: StateFlow<List<SteerDevice>?>? = null
    /**
     * The online machines this surface can start on: the caller's own plus
     * (EXP-432) the selected team's shared servers. null = not resolved yet.
     * Empty until [attach].
     */
    val devices: StateFlow<List<SteerDevice>?>
        get() = _devices ?: noDevices

    private var _registry: StateFlow<List<SteerDevice>?>? = null
    /** EXP-836: the WHOLE registry behind [devices], offline rows included. */
    val registry: StateFlow<List<SteerDevice>?>
        get() = _registry ?: noDevices

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    /** The session the last accepted send landed in — consumed exactly once. */
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    private var scope: CoroutineScope? = null

    private val noCandidates = MutableStateFlow<List<IssueOption>>(emptyList())
    private var _startCandidates: StateFlow<List<IssueOption>>? = null

    /**
     * Issues the composer's picker can chip: the selected team's live boards'
     * open issues, `updatedAt` desc. Empty until [attach].
     */
    val startCandidates: StateFlow<List<IssueOption>>
        get() = _startCandidates ?: noCandidates

    /** Bind to the hosting ViewModel's scope — call once from its `init`. */
    fun attach(
        scope: CoroutineScope,
        /** EXP-1186: the host's team (default: the selection). */
        teamId: Flow<String?>? = null,
    ) {
        if (this.scope != null) return
        this.scope = scope
        if (teamId != null) teamIdFlow = teamId
        _startCandidates = combine(
            dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
            dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
            teamIdFlow,
        ) { issues, boards, teamId ->
            if (teamId == null) {
                emptyList()
            } else {
                val eligibleBoards = boards
                    .filter { it.teamId == teamId && it.deletedAt == null }
                    .associateBy { it.id }
                issues
                    .filter { it.boardId in eligibleBoards.keys && it.status !in TERMINAL_ISSUE_STATUSES }
                    .sortedByDescending { it.updatedAt }
                    .map { issue ->
                        IssueOption(
                            id = issue.id,
                            identifier = issue.identifier,
                            title = issue.title,
                            repositoryId = eligibleBoards[issue.boardId]?.repositoryId,
                            status = issue.status,
                            priority = issue.priority,
                            description = issue.description,
                            createdAt = issue.createdAt,
                            updatedAt = issue.updatedAt,
                        )
                    }
            }
        }.stateIn(scope, SharingStarted.WhileSubscribed(5_000), emptyList())

        val registryFlow = steerDeviceFlow(dbFlow, teamIdFlow, auth.userId)
            .stateIn(scope, SharingStarted.WhileSubscribed(5_000), null)
        _registry = registryFlow
        _devices = combine(registryFlow, _enabled) { devices, enabled ->
            onlineStartTargets(devices, enabled)
        }.stateIn(scope, SharingStarted.WhileSubscribed(5_000), null)

        // Steer availability, resolved once per account (env-derived, static
        // per INSTANCE).
        scope.launch {
            var configuredAccountId: String? = null
            auth.activeAccountId.collectLatest { accountId ->
                _runState.value = ActionRunState.Idle
                if (accountId == null) {
                    configuredAccountId = null
                    _enabled.value = false
                    return@collectLatest
                }
                if (configuredAccountId != accountId) {
                    _enabled.value = null
                    _enabled.value = runCatching { steerApi.config(accountId).enabled }
                        .getOrDefault(false)
                    configuredAccountId = accountId
                }
            }
        }
    }

    fun consumeStartedSession() {
        _startedSessionId.value = null
    }

    /** Drop a stale Failed caption once the composer has shown it. */
    fun clearFailure() {
        if (_runState.value is ActionRunState.Failed) _runState.value = ActionRunState.Idle
    }

    /**
     * THE send: `sessions.send` with [input]. Returns the session it landed
     * in (also published as [startedSessionId]) or null on failure (the
     * message lands in [runState] as Failed — the host toasts it). The caller
     * clears its draft only on success.
     */
    suspend fun send(input: SessionsSendInput, deviceLabel: String = ""): String? {
        _runState.value = ActionRunState.Sending
        return try {
            val result = sender.send(input)
            _runState.value = if (deviceLabel.isNotBlank()) ActionRunState.Sent(deviceLabel) else ActionRunState.Idle
            _startedSessionId.value = result.sessionId
            result.sessionId
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            _runState.value = ActionRunState.Failed(trpcErrorMessage(t, "Couldn't send"))
            null
        }
    }
}

// Terminal issue statuses ineligible to start a new session.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
