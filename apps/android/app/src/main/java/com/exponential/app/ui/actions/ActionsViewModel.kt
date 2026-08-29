package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.AutomationsApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.toActionDto
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.domain.resumeTargetFor
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.onlineStartTargets
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

// The Actions surface (EXP-253, mobile = view + run only): the selected
// team's action prompts LIVE from the synced actions shape (EXP-268 — the
// local Room flow, body-less by design; the virtual "Fix merge conflicts"
// builtin is prepended client-side, while "Create action" hides behind the
// screen's "New action" button since EXP-431) plus the remote-run flow.
// After the server accepts ANY start — an action run or an issue/batch run
// off the sheet's Issues tab (EXP-536) — the model watches the synced
// coding_sessions DAO flow for the row the desktop inserts (StartedRunMatch
// owns the matching rules) and surfaces its id exactly once so the screen can
// jump into the existing agent session viewer.

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
    private val automationsApi: AutomationsApi,
    private val selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The online machines a run can go to: the caller's own plus (EXP-432) the
    // selected team's shared servers, filtered to ONLINE so the flow keeps the
    // presence-only semantics the screen gates on. Off the synced devices
    // shape since EXP-485. null = not resolved yet.
    val devices: StateFlow<List<SteerDevice>?> = combine(
        steerDeviceFlow(dbFlow, selection.selectedId, auth.userId),
        _steerEnabled,
    ) { devices, enabled -> onlineStartTargets(devices, enabled) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _runState = MutableStateFlow<ActionRunState>(ActionRunState.Idle)
    val runState: StateFlow<ActionRunState> = _runState

    // The freshly-started run's coding session id — consumed exactly once by
    // the screen's navigation (consumeStartedSession).
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    // The live rows the post-send watch scans — a start is only a command;
    // the desktop writes the coding_sessions row a moment later.
    private val liveSessionRows = dbFlow.scopedQuery(emptyList()) {
        it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
    }

    // The selected team, for the screen's "New action" entry point (EXP-431).
    val selectedTeamId: StateFlow<String?> = selection.selectedId

    // ── Automations tab (EXP-583) ────────────────────────────────────────────

    /**
     * EVERY synced device as a [SteerDevice], offline included — the
     * Automations tab resolves a trigger's bound deviceId to a label + online
     * dot, and an offline machine must still be nameable. Online-ness derives
     * from `last_seen_at` freshness, recomputed on the 30s ticker (Room flows
     * only re-emit on writes).
     */
    val syncedDevices: StateFlow<List<SteerDevice>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
        DeviceLiveness.ticker(),
        auth.userId,
    ) { rows, nowMs, userId ->
        rows.sortedWith(stableDeviceOrder(nowMs)).map { it.toSteerDevice(nowMs, userId) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The machines an automation can be bound to: every synced device that
     * advertises the `automations` cap, ONLINE OR NOT (an automation outlives
     * a machine's uptime — it fires whenever that machine is next up). */
    val automationDevices: StateFlow<List<SteerDevice>> = syncedDevices
        .map { devices -> devices.filter { it.canRunAutomations } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The selected team's automations, live off the synced shape (EXP-583). */
    val automations: StateFlow<List<AutomationEntity>> =
        combine(dbFlow, selection.selectedId) { db, teamId ->
            db to teamId
        }.flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(emptyList())
            } else {
                db.automationDao().observeByTeam(teamId)
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Every coding_sessions row of the selected team, newest first — the
    // source both automation lists derive from (one DAO flow, not two).
    private val teamSessions: StateFlow<List<CodingSessionEntity>> =
        combine(dbFlow, selection.selectedId) { db, teamId ->
            db to teamId
        }.flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(emptyList())
            } else {
                db.codingSessionDao().observeByTeam(teamId)
                    .map { rows -> rows.sortedByDescending { it.startedAt } }
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Recent automation-started runs (coding_sessions rows with a non-null
     * `started_reason`), newest first, capped at 10 for the "Recent automated
     * runs" list.
     */
    val automationRuns: StateFlow<List<CodingSessionEntity>> = teamSessions
        .map { rows -> rows.filter { it.startedReason != null }.take(10) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-637: where a Resume would go for each listed automated run, keyed by
     * session id. Only the caller's OWN ended runs whose machine is still
     * online and advertises `resume-run` resolve — everything else simply
     * shows no Resume.
     */
    val runResumeTargets: StateFlow<Map<String, RunResumeTarget>> = combine(
        automationRuns,
        syncedDevices,
        auth.userId,
    ) { runs, devices, userId ->
        runs.mapNotNull { resumeTargetFor(it, devices, userId) }.associateBy { it.sessionId }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    // Session ids with a resume in flight — the row swaps its Resume pill for
    // a spinner until the desktop's new row lands (or the watch gives up).
    private val _resuming = MutableStateFlow<Set<String>>(emptySet())
    val resuming: StateFlow<Set<String>> = _resuming

    /** The newest run each automation produced — the rows' "Last run" line.
     * Keyed by `automation_id`, which only automated runs carry. */
    val lastRunByAutomation: StateFlow<Map<String, CodingSessionEntity>> = teamSessions
        .map { rows ->
            val byAutomation = LinkedHashMap<String, CodingSessionEntity>()
            rows.forEach { session ->
                val id = session.automationId ?: return@forEach
                if (id !in byAutomation) byAutomation[id] = session
            }
            byAutomation
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /**
     * Whether the current user OWNS the selected team — the Automations
     * enabled toggle, the "New automation" button and Delete are hidden or
     * disabled otherwise (the `automations` writes are owner-gated
     * server-side; mirror it instead of bouncing on submit).
     */
    val isTeamOwner: StateFlow<Boolean> = combine(dbFlow, selection.selectedId) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(emptyList())
        } else {
            db.teamMemberDao().observeByTeam(teamId)
        }
    }.combine(auth.userId) { members, userId ->
        userId != null &&
            members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    // True while ANY automation mutation is in flight — every switch and the
    // form's submit park together (one mutation at a time, iOS parity).
    private val _automationBusy = MutableStateFlow(false)
    val automationBusy: StateFlow<Boolean> = _automationBusy

    private val _automationError = MutableStateFlow<String?>(null)
    val automationError: StateFlow<String?> = _automationError

    fun clearAutomationError() {
        _automationError.value = null
    }

    /** Flip an automation's paused flag — `automations.update({id, enabled})`.
     * Success needs no local write: Electric echoes the row back. */
    fun setAutomationEnabled(automationId: String, enabled: Boolean) {
        mutateAutomation("The automation could not be updated") { accountId ->
            automationsApi.setEnabled(accountId, id = automationId, enabled = enabled)
        }
    }

    /**
     * Save an edited automation (EXP-615) — the same form as create, on an
     * existing row. Null [agent]/[model]/[effort] clear the pins back to the
     * bound machine's own launch defaults.
     */
    fun updateAutomation(
        automationId: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        agent: String?,
        model: String?,
        effort: String?,
        onDone: () -> Unit,
    ) {
        mutateAutomation("The automation could not be updated", onDone) { accountId ->
            automationsApi.update(
                accountId,
                id = automationId,
                actionId = actionId,
                deviceId = deviceId,
                trigger = trigger,
                agent = agent,
                model = model,
                effort = effort,
            )
        }
    }

    /** Owner-only, permanent — the row vanishes when Electric echoes it. */
    fun deleteAutomation(automationId: String) {
        mutateAutomation("The automation could not be deleted") { accountId ->
            automationsApi.delete(accountId, automationId)
        }
    }

    /**
     * Create an automation from the form sheet. Null [agent]/[model]/[effort]
     * mean the bound machine's own launch defaults; [onDone] fires only on
     * success, so the sheet stays open (with the error) on a refusal.
     */
    fun createAutomation(
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        agent: String?,
        model: String?,
        effort: String?,
        onDone: () -> Unit,
    ) {
        val teamId = selection.selectedId.value ?: return
        mutateAutomation("The automation could not be created", onDone) { accountId ->
            automationsApi.create(
                accountId,
                teamId = teamId,
                actionId = actionId,
                deviceId = deviceId,
                trigger = trigger,
                agent = agent,
                model = model,
                effort = effort,
            )
        }
    }

    // One mutation at a time, with the server's own refusal message surfaced
    // (the server's copy is the actionable one — required inputs, an
    // incapable device, a rejected model).
    private fun mutateAutomation(
        fallback: String,
        onDone: () -> Unit = {},
        block: suspend (String) -> Unit,
    ) {
        if (_automationBusy.value) return
        _automationBusy.value = true
        _automationError.value = null
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _automationBusy.value = false
                return@launch
            }
            try {
                block(accountId)
                onDone()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _automationError.value = trpcErrorMessage(t, fallback)
            }
            _automationBusy.value = false
        }
    }

    // Live from the synced actions shape (EXP-268): the DAO orders by
    // sort_order then name. NEITHER client builtin is listed (EXP-431,
    // EXP-686): "Create action" lives behind the header's "New action" button,
    // and "Fix merge conflicts" is launched from Reviews / the start-coding
    // sheet (which builds its own list) — neither poses as a team action here.
    val state: StateFlow<ActionsState> =
        combine(dbFlow, selection.selectedId) { db, teamId ->
            db to teamId
        }.flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(ActionsState(loading = false))
            } else {
                db.actionDao().observeByTeam(teamId).map { rows ->
                    ActionsState(
                        actions = rows.map { it.toActionDto(json) },
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
        // Steer availability, resolved once per account (the AgentsViewModel
        // pattern): it is env-derived and static per instance, and the machines
        // themselves now come from sync rather than a per-team fetch.
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _runState.value = ActionRunState.Idle
                if (accountId == null) {
                    configuredAccountId = null
                    _steerEnabled.value = false
                    return@collectLatest
                }
                if (configuredAccountId != accountId) {
                    _steerEnabled.value = null
                    _steerEnabled.value = runCatching { steerApi.config(accountId).enabled }
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
     * Remote-start issues from the unified sheet's Issues tab (the
     * AgentsViewModel.startCoding twin, surfaced through the run captions):
     * 1 id launches a plain single session, 2+ a batch. EXP-536: both wait
     * for the desktop's row and jump into it, exactly like an action run.
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        val key = StartedRunKey.forIssues(issueIds) ?: return
        viewModelScope.launch {
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
     * EXP-637: continue an ENDED run on the machine that ran it — the agent
     * picks up in the same workspace with its own transcript. A resumed run
     * keeps its recorded agent and options, so nothing else rides along; the
     * desktop's new row is matched by its `resumed_from_id`.
     */
    fun resumeRun(target: RunResumeTarget) {
        if (target.sessionId in _resuming.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _resuming.value = _resuming.value + target.sessionId
            _runState.value = ActionRunState.Sending
            try {
                steerApi.resumeSession(accountId, target.sessionId, target.deviceId)
                awaitStartedRun(StartedRunKey.Resumed(target.sessionId), target.deviceLabel)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _runState.value = ActionRunState.Failed(
                    trpcErrorMessage(t, "The run could not be resumed"),
                )
            } finally {
                _resuming.value = _resuming.value - target.sessionId
            }
        }
    }

    /**
     * Hold the "waiting for the desktop" caption until the run's synced
     * coding_sessions row appears (then hand it to the screen's navigation),
     * or until the deadline passes. A start the desktop REFUSED — conflicted
     * worktree, failed doctor — would otherwise leave the caption up forever
     * (EXP-536); the desktop holds the reason, so say where to look.
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
