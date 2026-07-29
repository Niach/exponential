package com.exponential.app.ui.steer

import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.issue.StartIssueOption
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

// Remote-start plumbing shared by the screens that host the unified
// StartCodingSheet without owning an agents surface of their own (EXP-323:
// Reviews and the Changes tab, whose "Fix conflicts" buttons launch the
// builtin action). Steer availability + device presence + the issue candidate
// pool + the post-start session watch are screen-agnostic, so they live here
// instead of being copy-pasted into another ViewModel. The older hosts
// (Actions, Agents, IssueList, IssueDetail) still carry their own copies.

/** Run feedback: an informational Sent caption vs a persistent red Failed. */
sealed interface ActionRunState {
    data object Idle : ActionRunState
    data object Sending : ActionRunState
    data class Sent(val deviceLabel: String) : ActionRunState
    data class Failed(val message: String) : ActionRunState
}

@OptIn(ExperimentalCoroutinesApi::class)
class SteerLaunchDelegate @Inject constructor(
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    holder: DatabaseHolder,
    selection: TeamSelection,
) {

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val teamIdFlow = selection.selectedId

    private val _enabled = MutableStateFlow<Boolean?>(null)
    /** Steer availability on this instance. null = not resolved yet. */
    val enabled: StateFlow<Boolean?> = _enabled

    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    /** The caller's online desktops (relay presence). null = not loaded yet. */
    val devices: StateFlow<List<SteerDevice>?> = _devices

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    /** The freshly-started run's coding session id — consumed exactly once. */
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    private var scope: CoroutineScope? = null
    private var watchJob: Job? = null

    private val noCandidates = MutableStateFlow<List<StartIssueOption>>(emptyList())
    private var _startCandidates: StateFlow<List<StartIssueOption>>? = null

    /**
     * Issues the sheet's Issues tab can queue (the AgentsViewModel candidate
     * rules): the selected team's repo-backed, live boards; open issues,
     * `updatedAt` desc. Empty until [attach].
     */
    val startCandidates: StateFlow<List<StartIssueOption>>
        get() = _startCandidates ?: noCandidates

    /** Bind to the hosting ViewModel's scope — call once from its `init`. */
    fun attach(scope: CoroutineScope) {
        if (this.scope != null) return
        this.scope = scope
        _startCandidates = combine(
            dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
            dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
            teamIdFlow,
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
        }.stateIn(scope, SharingStarted.WhileSubscribed(5_000), emptyList())

        // Steer availability + device presence, re-fetched on account switch.
        scope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _enabled.value = null
                _devices.value = null
                _runState.value = ActionRunState.Idle
                if (accountId == null) {
                    _enabled.value = false
                    _devices.value = emptyList()
                    return@collectLatest
                }
                val on = runCatching { steerApi.config(accountId).enabled }.getOrDefault(false)
                _enabled.value = on
                _devices.value = if (on) {
                    runCatching { steerApi.myDevices(accountId).devices }.getOrDefault(emptyList())
                } else {
                    emptyList()
                }
            }
        }
    }

    /** Re-poll device presence (on screen resume) — no-op until steer resolves on. */
    fun refreshDevices() {
        if (_enabled.value != true) return
        val scope = scope ?: return
        scope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { steerApi.myDevices(accountId).devices }
                .onSuccess { _devices.value = it }
        }
    }

    fun consumeStartedSession() {
        _startedSessionId.value = null
    }

    /**
     * Remote-run [action] on [device] with the sheet's full [options] + filled
     * [inputs], then watch the synced coding_sessions flow for the desktop's
     * row. EVERY builtin additionally rides its teamId (there is no DB row to
     * derive the team from); the server rejects it on a non-builtin.
     */
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) {
        val scope = scope ?: return
        scope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _runState.value = ActionRunState.Sending
            try {
                steerApi.startActionSession(
                    accountId,
                    actionId = action.id,
                    deviceId = device.deviceId,
                    options = options,
                    teamId = action.teamId.takeIf { action.isBuiltin },
                    inputs = inputs.takeIf { it.isNotEmpty() },
                )
                val label = device.deviceLabel.ifBlank { device.deviceId }
                _runState.value = ActionRunState.Sent(label)
                watchForStartedRun(action.name, auth.userId.value)
                // Keep the Sent caption for the whole watch deadline — a slow
                // desktop pickup can still navigate late, and a captionless
                // late jump reads as a glitch.
                delay(WATCH_DEADLINE_MS)
                if (_runState.value is ActionRunState.Sent) {
                    // The deadline passed with no session row (EXP-357). This
                    // used to fall back to Idle, so a run the desktop REFUSED
                    // — a conflicted worktree, a doctor failure — was
                    // indistinguishable from one still starting: the caption
                    // just vanished and nothing ever appeared. The desktop
                    // holds the reason (it notifies there); say so.
                    _runState.value = ActionRunState.Failed(
                        "$label never started this run — open the Exponential desktop app " +
                            "there to see why.",
                    )
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /** Remote-start issues from the sheet's Issues tab: 1 id plain, 2+ a batch. */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        if (issueIds.isEmpty()) return
        val scope = scope ?: return
        scope.launch {
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
    // the only stable key), the caller's own userId, and a startedAt after the
    // send (with clock-skew slack). Gives up silently after a deadline.
    private fun watchForStartedRun(actionName: String, userId: String?) {
        watchJob?.cancel()
        val scope = scope ?: return
        if (userId == null) return
        val cutoffMs = System.currentTimeMillis() - 120_000
        watchJob = scope.launch {
            val match = withTimeoutOrNull(WATCH_DEADLINE_MS) {
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

// How long a remote action run may take to surface its coding_sessions row
// before the caption calls it dead. The session watch and the caption share
// it — a caption that outlives the watch would keep spinning forever, one
// that dies first would strand a late navigation with no explanation.
private const val WATCH_DEADLINE_MS = 180_000L
