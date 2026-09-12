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
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.ui.agent.IssueOption
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

// Remote-start plumbing (EXP-323): steer availability + device presence + the
// issue candidate pool + the post-start session watch. EXP-825: the Agent page
// composer is the ONE launcher, so this is composed by AgentComposerViewModel
// (starts, action runs) and by AgentSessionViewModel (Resume) — the per-screen
// copies the old Start-coding sheet's hosts carried are gone with the sheet.

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

    private var _registry: StateFlow<List<SteerDevice>?>? = null
    /**
     * EXP-836: the WHOLE registry behind [devices] — offline rows included, so
     * a launcher can say why the machine a play button named is not the one its
     * run would go to ("offline" reads differently from "gone"). null until the
     * devices shape's first snapshot, like [devices].
     */
    val registry: StateFlow<List<SteerDevice>?>
        get() = _registry ?: noDevices

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

    private val noCandidates = MutableStateFlow<List<IssueOption>>(emptyList())
    private var _startCandidates: StateFlow<List<IssueOption>>? = null

    /**
     * Issues the composer's picker can chip: the selected team's repo-backed,
     * live boards; open issues, `updatedAt` desc. Empty until [attach].
     */
    val startCandidates: StateFlow<List<IssueOption>>
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
                        IssueOption(
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
        val registryFlow = steerDeviceFlow(dbFlow, teamIdFlow, auth.userId)
            .stateIn(scope, SharingStarted.WhileSubscribed(5_000), null)
        _registry = registryFlow
        _devices = combine(registryFlow, _enabled) { devices, enabled ->
            onlineStartTargets(devices, enabled)
        }.stateIn(scope, SharingStarted.WhileSubscribed(5_000), null)

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
     * Remote-run [action] on [device] with the composer's full [options] +
     * filled [inputs] and its [prompt] (EXP-825: the chat message, the
     * create-action request, or additional instructions). EVERY builtin
     * additionally rides its teamId (there is no DB row to derive the team
     * from); the server rejects it on a non-builtin. Returns whether the SEND
     * was accepted — the caller clears its draft only then — and, on success,
     * watches the synced coding_sessions flow for the desktop's row in the
     * background.
     */
    suspend fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
        prompt: String? = null,
    ): Boolean {
        val scope = scope ?: return false
        val accountId = auth.activeAccountId.value ?: return false
        _runState.value = ActionRunState.Sending
        try {
            steerApi.startActionSession(
                accountId,
                actionId = action.id,
                deviceId = device.deviceId,
                options = options,
                teamId = action.teamId.takeIf { action.isBuiltin },
                inputs = inputs.takeIf { it.isNotEmpty() },
                prompt = prompt,
            )
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            _runState.value = ActionRunState.Failed(
                trpcErrorMessage(t, "The start command could not be delivered"),
            )
            return false
        }
        scope.launch { awaitStartedRun(StartedRunKey.Action(action.name), device) }
        return true
    }

    /**
     * Remote-start issues off the composer's chips: 1 id plain, 2+ a batch,
     * [prompt] riding as additional instructions (EXP-825). Same send/watch
     * contract as [runAction]: EXP-536 waits for the desktop's row and
     * surfaces it as [startedSessionId], so the host screen opens the live
     * session.
     */
    suspend fun startIssues(
        device: SteerDevice,
        issueIds: List<String>,
        options: SteerStartOptions,
        prompt: String? = null,
    ): Boolean {
        val key = StartedRunKey.forIssues(issueIds) ?: return false
        val scope = scope ?: return false
        val accountId = auth.activeAccountId.value ?: return false
        _runState.value = ActionRunState.Sending
        try {
            if (issueIds.size >= 2) {
                steerApi.startSession(accountId, issueIds, device.deviceId, options, prompt)
            } else {
                steerApi.startSession(accountId, issueIds.first(), device.deviceId, options, prompt)
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            _runState.value = ActionRunState.Failed(
                trpcErrorMessage(t, "The start command could not be delivered"),
            )
            return false
        }
        scope.launch { awaitStartedRun(key, device) }
        return true
    }

    /** Drop a stale Failed caption once the composer has shown it. */
    fun clearFailure() {
        if (_runState.value is ActionRunState.Failed) _runState.value = ActionRunState.Idle
    }

    /**
     * EXP-773: pick an ENDED run up again on the machine that ran it. Same
     * command the list rows used to send, moved here with the affordance: the
     * desktop relaunches the pinned agent in the run's own worktree and
     * inserts a NEW session row, which this then hands to the host screen.
     *
     * EXP-849 phase 3: [account] names a LOGIN to re-enter under — the
     * mid-session switch, which is the same resume with an account on it
     * (claude only). The machine ends the live run, re-enters the recorded one
     * under that login and inserts the continuation row, which lands in
     * [startedSessionId] exactly like a resume's does, so the screen follows
     * the new run. Null = the plain Resume, on the run's own account.
     */
    fun resumeRun(target: RunResumeTarget, account: String? = null) {
        val scope = scope ?: return
        scope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _runState.value = ActionRunState.Sending
            try {
                steerApi.resumeSession(accountId, target.sessionId, target.deviceId, account)
                awaitStartedRun(
                    StartedRunKey.Resumed(target.sessionId),
                    target.deviceLabel,
                )
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(
                        t,
                        if (account == null) {
                            "The run could not be resumed"
                        } else {
                            "The account could not be switched"
                        },
                    ),
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
    private suspend fun awaitStartedRun(key: StartedRunKey, device: SteerDevice) =
        awaitStartedRun(key, device.deviceLabel.ifBlank { device.deviceId })

    private suspend fun awaitStartedRun(key: StartedRunKey, label: String) {
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
