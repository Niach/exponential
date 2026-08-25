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
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.ui.issue.StartIssueOption
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

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

    private val noDevices = MutableStateFlow<List<SteerDevice>?>(null)
    private var _devices: StateFlow<List<SteerDevice>?>? = null
    /**
     * The online machines this surface can start on: the caller's own plus
     * (EXP-432) the selected team's shared servers, filtered to ONLINE so the
     * flow keeps the presence-only semantics its callers gate on. null = not
     * resolved yet. Empty until [attach].
     */
    val devices: StateFlow<List<SteerDevice>?>
        get() = _devices ?: noDevices

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    /** The freshly-started run's coding session id — consumed exactly once. */
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    private var scope: CoroutineScope? = null

    // The live rows the post-send watch scans — a start is only a command;
    // the desktop writes the coding_sessions row a moment later.
    private val liveSessionRows = dbFlow.scopedQuery(emptyList()) {
        it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
    }

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

        // The team-scoped registry (EXP-432) narrowed to what can take a start
        // right now — off the synced shape since EXP-485, so a team switch
        // re-scopes it without a round trip.
        _devices = combine(
            steerDeviceFlow(dbFlow, teamIdFlow, auth.userId),
            _enabled,
        ) { devices, enabled -> onlineStartTargets(devices, enabled) }
            .stateIn(scope, SharingStarted.WhileSubscribed(5_000), null)

        // Steer availability, resolved once per account: it is env-derived and
        // static per INSTANCE, so re-running it would blank `enabled` and
        // flicker the hosting screen's start affordances.
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
                awaitStartedRun(StartedRunKey.Action(action.name), device)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * Remote-start issues from the sheet's Issues tab: 1 id plain, 2+ a
     * batch. EXP-536: both then wait for the desktop's row and surface it as
     * [startedSessionId], so the host screen opens the live session.
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        val key = StartedRunKey.forIssues(issueIds) ?: return
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
                awaitStartedRun(key, device)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * Hold the "waiting for the desktop" caption until the run's synced
     * coding_sessions row appears (then hand it to the host screen's
     * navigation), or until the deadline passes. The deadline is NOT a silent
     * fall back to Idle: a run the desktop REFUSED — a conflicted worktree, a
     * doctor failure — would be indistinguishable from one still starting,
     * the caption would just vanish and nothing would ever appear (EXP-357).
     * The desktop holds the reason (it notifies there); say so.
     */
    private suspend fun awaitStartedRun(key: StartedRunKey, device: SteerDevice) {
        val label = device.deviceLabel.ifBlank { device.deviceId }
        _runState.value = ActionRunState.Sent(label)
        val userId = auth.userId.value
        val sessionId = if (userId == null) {
            null
        } else {
            StartedRunMatch.await(liveSessionRows, key, userId)
        }
        if (sessionId != null) {
            _runState.value = ActionRunState.Idle
            _startedSessionId.value = sessionId
        } else {
            _runState.value = ActionRunState.Failed(
                "$label never started this run. Open the Exponential desktop app " +
                    "there to see why.",
            )
        }
    }
}

// Terminal issue statuses ineligible to start a new coding run.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
