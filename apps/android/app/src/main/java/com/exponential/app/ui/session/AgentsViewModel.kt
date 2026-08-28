package com.exponential.app.ui.session

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DeviceFreshness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.resumeTargetFor
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.issue.SteerStartState
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
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The Agents tab: the signed-in user's OWN coding sessions currently running
// (synced coding_sessions shape joined to its issue), plus a remote-start
// launcher against the user's machines (EXP-156). The desktop remains the only
// session runner — this tab lists live sessions and kicks off new (single or
// batch) runs on a picked machine.
//
// The machine list is the SYNCED devices shape (EXP-481 — the EXP-403
// registry became server-authoritative synced state): own rows plus (EXP-432)
// teammates' server machines shared with the selected team, with online-ness
// derived client-side from last_seen_at freshness on a 30s ticker. The 15s
// devices.list polling died with the shape, and EXP-485 retired the procedure
// itself — only the informational `devices.latestVersions` query is still
// fetched (one per account).

data class AgentRow(
    val session: CodingSessionEntity,
    val issue: IssueEntity?,
    // EXP-549/550: the host machine resolved against its LIVE devices row —
    // the CURRENT label (not the start-time snapshot) plus whether the machine
    // dropped offline, which renders the row as paused rather than live.
    val device: SessionDevicePresentation = SessionDevicePresentation.Unknown,
    // EXP-535: a batch session's resolved open PR, as a representative linked
    // issue (merging through it merges the ONE batch PR — Reviews pattern).
    // Set only on issueless batch rows in review with an UNAMBIGUOUS match.
    val batchPrIssue: IssueEntity? = null,
)

data class AgentsState(
    val rows: List<AgentRow> = emptyList(),
    // steer.config is env-derived and static per instance: null = still
    // loading. Decides whether a row tap opens the live viewer directly or
    // falls back to the issue detail, and whether the devices section shows.
    val steerEnabled: Boolean? = null,
)

/**
 * EXP-637: one FINISHED run in the "Recent runs" list — the session row, its
 * issue when it had one, and the machine that ran it (for the byline label).
 */
data class RecentRunRow(
    val session: CodingSessionEntity,
    val issue: IssueEntity?,
    val device: SessionDevicePresentation = SessionDevicePresentation.Unknown,
    // Where a Resume would go, or null when the run can't be resumed right
    // now (its machine is gone, offline, or too old to know how).
    val resume: RunResumeTarget? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val issuesApi: IssuesApi,
    private val selection: TeamSelection,
    stats: SyncStats,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** EXP-656: when our own `devices` shape last completed a poll — a cursor
     *  we haven't refreshed can only produce a FALSE offline, so a session on
     *  one must render unknown presence rather than "Paused". */
    private val devicesPolledAt = auth.activeAccountId.flatMapLatest { stats.devicesPolledAt(it) }

    // The machine rows every session row joins to (EXP-549/550: current labels
    // and last_seen_at freshness), paired with that freshness stamp.
    private val deviceRowsAndFreshness = combine(
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        devicesPolledAt,
    ) { devices, polledAt -> devices to polledAt }

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's registered machines, online AND offline, from the synced
    // devices shape — plus the selected team's shared servers. null until the
    // shape's initial snapshot has landed (offset is_live), so the section
    // shows nothing rather than a flash of "No machines yet".
    val devices: StateFlow<List<SteerDevice>?> =
        steerDeviceFlow(dbFlow, selection.selectedId, auth.userId)
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Informational CLIENT_LATEST_VERSION_* values behind the "update
    // available" hint on a machine row; both null until the first list lands.
    private val _latestVersions = MutableStateFlow(DeviceLatestVersions())
    val latestVersions: StateFlow<DeviceLatestVersions> = _latestVersions

    // Machine ids with a rename/remove/update mutation in flight — the row's
    // menu stays put but its actions disable until the refetch lands.
    private val _deviceBusy = MutableStateFlow<Set<String>>(emptySet())
    val deviceBusy: StateFlow<Set<String>> = _deviceBusy

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    // EXP-536: the freshly-started run's session id — consumed exactly once
    // by the screen, which opens the live viewer on it.
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    // The live rows the post-send watch scans (the same DAO flow the list
    // renders from — a start is only a command; the desktop writes the row).
    private val liveSessionRows = dbFlow.scopedQuery(emptyList()) {
        it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
    }

    // Bundled up front: the typed `combine` overloads stop at five flows, and
    // the state below already needs seven. Boards ride along for the batch-PR
    // resolution (EXP-535) — issues don't sync team_id, so team scoping goes
    // through their board.
    private val liveSessionsIssuesAndBoards = combine(
        liveSessionRows,
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
    ) { sessions, issues, boards -> Triple(sessions, issues, boards) }

    // Bundled to keep the combine below inside the typed overloads.
    private val steerEnabledAndDevices = combine(
        _steerEnabled,
        deviceRowsAndFreshness,
    ) { steerEnabled, (devices, polledAt) -> Triple(steerEnabled, devices, polledAt) }

    val state: StateFlow<AgentsState> = combine(
        liveSessionsIssuesAndBoards,
        steerEnabledAndDevices,
        // Heartbeat-stale rows render as absent (EXP-153); the ticker clears
        // them once the liveness window elapses without a sync delta. The
        // DEVICE window is only 90s (EXP-550), so this ticks at its 30s
        // cadence — a minute tick could lag the paused flip by two-thirds of
        // the window.
        DeviceLiveness.ticker(),
        auth.userId,
        selection.selectedId,
    ) { (sessions, issues, boards), (steerEnabled, devices, polledAt), now, userId, teamId ->
        AgentsState(
            rows = agentRows(
                sessions, issues, boards, userId, teamId, now, devices,
                // The stamp rides elapsedRealtime, not the wall clock `now`.
                devicesFresh = DeviceFreshness.isTrustworthy(
                    polledAt,
                    SystemClock.elapsedRealtime(),
                ),
            ),
            steerEnabled = steerEnabled,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

    // EXP-637: the caller's own FINISHED sessions in the selected team, newest
    // first — the source of the "Recent runs" list. Queried wider than the
    // list shows: the pure filter keeps only the agent's own close-outs, and a
    // run killed or merged in between must not push a real close-out off the
    // end.
    private val endedSessionRows = combine(
        dbFlow,
        selection.selectedId,
        auth.userId,
    ) { db, teamId, userId -> Triple(db, teamId, userId) }
        .flatMapLatest { (db, teamId, userId) ->
            if (db == null || teamId == null || userId == null) {
                flowOf(emptyList())
            } else {
                db.codingSessionDao().observeRecentByTeamAndUser(
                    teamId = teamId,
                    userId = userId,
                    status = DomainContract.codingSessionStatusEnded,
                    limit = RECENT_RUN_QUERY_LIMIT,
                )
            }
        }

    /**
     * EXP-637: the finished runs the agent closed out itself, newest first —
     * each expandable to its summary and (on a capable, online machine) a
     * Resume. Empty renders nothing at all.
     */
    val recentRuns: StateFlow<List<RecentRunRow>> = combine(
        endedSessionRows,
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        deviceRowsAndFreshness,
        combine(auth.userId, selection.selectedId) { userId, teamId -> userId to teamId },
        DeviceLiveness.ticker(),
    ) { sessions, issues, (devices, polledAt), (userId, teamId), now ->
        recentRunRows(
            sessions, issues, userId, teamId, devices, now,
            devicesFresh = DeviceFreshness.isTrustworthy(polledAt, SystemClock.elapsedRealtime()),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Session ids with a resume in flight — the row swaps its Resume pill for
    // a spinner until the desktop's new row lands (or the watch gives up).
    private val _resuming = MutableStateFlow<Set<String>>(emptySet())
    val resuming: StateFlow<Set<String>> = _resuming

    // Issues the Start-coding sheet can queue, scoped to the SELECTED team
    // (no current-issue exemption here — this tab has no "current" issue):
    // repo-backed, live boards; open issues, `updatedAt` desc.
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

    // The account steer.config was last resolved for. steer.config is
    // env-derived and static per INSTANCE, so a team switch must not re-run it
    // — that would blank `steerEnabled` and flicker the whole tab (EXP-432).
    private var configuredAccountId: String? = null

    init {
        // Steer availability + the machine registry, re-fetched on account
        // switch (mirrors the issue detail's check) and, since EXP-432, on
        // team switch — the shared machines the list carries belong to the
        // SELECTED team.
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _latestVersions.value = DeviceLatestVersions()
                _startState.value = SteerStartState.Idle
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
                // The rows come from sync — the only thing left to fetch is the
                // informational version floor (one query per account; a failure
                // just hides the update hint).
                if (_steerEnabled.value == true) {
                    runCatching { devicesApi.latestVersions(accountId) }
                        .onSuccess { _latestVersions.value = it }
                }
            }
        }
    }

    /** Clears the one-shot navigation signal once the screen has acted on it. */
    fun consumeStartedSession() {
        _startedSessionId.value = null
    }

    /** Rename a machine (its registry label wins over the relay's). */
    fun renameDevice(deviceId: String, label: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.rename(accountId, deviceId, label.trim()) }

    /**
     * Drop the registry row. A machine whose daemon still runs re-registers
     * itself on its next heartbeat — the confirm dialog says so.
     */
    fun removeDevice(deviceId: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.remove(accountId, deviceId) }

    /** Ask a server daemon to self-update; it restarts when idle. */
    fun requestDeviceUpdate(deviceId: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.requestUpdate(accountId, deviceId) }

    // Every row mutation follows the same shape: mark the row busy, run, and
    // let sync land the change (the mutations return txids the web awaits;
    // here the shape delta arrives within the long-poll).
    private fun mutateDevice(deviceId: String, block: suspend (String) -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _deviceBusy.value = _deviceBusy.value + deviceId
            runCatching { block(accountId) }
            _deviceBusy.value = _deviceBusy.value - deviceId
        }
    }

    /**
     * Remote-start on a picked desktop (EXP-156): [issueIds] of size 1 launches
     * a plain single session, 2+ a batch. EXP-536: both then WAIT for the
     * desktop's coding_sessions row and surface it as [startedSessionId], so
     * the screen opens the live session instead of listing it.
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        val key = StartedRunKey.forIssues(issueIds) ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
            try {
                if (issueIds.size >= 2) {
                    steerApi.startSession(accountId, issueIds, device.deviceId, options)
                } else {
                    steerApi.startSession(accountId, issueIds.first(), device.deviceId, options)
                }
                awaitStartedRun(key, device)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * Remote-run a team action from the unified sheet's Actions tab (EXP-257)
     * with the full option set + filled inputs; the builtin "Create action"
     * id additionally rides its teamId (server-required there, forbidden
     * otherwise). Same Sent/grace-window handling as [startCoding].
     */
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
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
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * EXP-637: continue an ENDED run on the machine that ran it — the agent
     * picks up in the same workspace with its own transcript. The resumed run
     * keeps its recorded agent and options, so nothing else rides along; the
     * new row is matched by its `resumed_from_id`, which is exact.
     */
    fun resumeRun(target: RunResumeTarget) {
        if (target.sessionId in _resuming.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _resuming.value = _resuming.value + target.sessionId
            _startState.value = SteerStartState.Sending
            try {
                steerApi.resumeSession(accountId, target.sessionId, target.deviceId)
                awaitStartedRun(StartedRunKey.Resumed(target.sessionId), target.deviceLabel)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The run could not be resumed"),
                )
            } finally {
                _resuming.value = _resuming.value - target.sessionId
            }
        }
    }

    /**
     * EXP-536: hold a "waiting for the desktop" caption until the run's
     * synced row appears (then hand it to the screen's navigation), or until
     * the deadline passes — a start the desktop REFUSED (conflicted worktree,
     * failed doctor) would otherwise just leave the caption hanging forever,
     * which is exactly the chip-never-disappears bug.
     */
    private suspend fun awaitStartedRun(key: StartedRunKey, device: SteerDevice) =
        awaitStartedRun(key, device.deviceLabel.ifBlank { device.deviceId })

    private suspend fun awaitStartedRun(key: StartedRunKey, label: String) {
        _startState.value = SteerStartState.Sent(label)
        val userId = auth.userId.value
        val sessionId = if (userId == null) {
            null
        } else {
            StartedRunMatch.await(liveSessionRows, key, userId)
        }
        if (sessionId != null) {
            _startState.value = SteerStartState.Idle
            _startedSessionId.value = sessionId
        } else {
            _startState.value = SteerStartState.Failed(
                "$label never started this run. Open the Exponential desktop app " +
                    "there to see why.",
            )
        }
    }

    // ── Merge (EXP-498: merging always closes the session) ──────────────────
    // The server merges AND ends the session, so the row drops off this list
    // on its own once the `ended` flip syncs. Keyed by issue id: several rows
    // can be in flight at once.
    private val _merging = MutableStateFlow<Set<String>>(emptySet())
    val merging: StateFlow<Set<String>> = _merging

    // Rendered INLINE on the failing row (EXP-323 pattern — a snackbar hides
    // behind the floating bottom nav pill). Cleared by the next attempt.
    private val _mergeErrors = MutableStateFlow<Map<String, String>>(emptyMap())
    val mergeErrors: StateFlow<Map<String, String>> = _mergeErrors

    /**
     * Squash-merge the row's PR — the server always ends its coding session
     * too (EXP-498). For a batch PR the server resolves [issueId] to ALL
     * linked issues and completes them together.
     */
    fun merge(issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeErrors.value = _mergeErrors.value - issueId
            _merging.value = _merging.value + issueId
            runCatching { issuesApi.mergePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // common, persistent failures of a squash merge — same copy
                    // as Reviews and the issue Changes tab.
                    _mergeErrors.value = _mergeErrors.value +
                        (issueId to trpcErrorMessage(t, "The pull request could not be merged"))
                }
            _merging.value = _merging.value - issueId
        }
    }
}

/**
 * The Agents list: the signed-in user's OWN live sessions in the SELECTED team
 * only. A teammate's live session is neither viewable nor steerable (EXP-312),
 * so listing it just read as "computer not online" — and a session in another
 * team belongs under that team, matching web's `use-agents-data.ts`. The rows
 * stay SYNCED for the issue-detail badges and Reviews, they only leave this
 * list. Every session row carries a non-null synced `team_id` (denormalized by
 * trigger for issue rows, explicit on batch/action rows), so the scoping holds
 * for issueless runs too. Signed out (null [currentUserId]) or no team
 * selected (null [teamId]) lists nothing.
 */
fun agentRows(
    sessions: List<CodingSessionEntity>,
    issues: List<IssueEntity>,
    boards: List<BoardEntity>,
    currentUserId: String?,
    teamId: String?,
    nowMs: Long,
    // EXP-549/550: the synced machine rows, for the live label + offline flip.
    // Defaulted so a caller that only cares about the session/issue join
    // (tests, and any future non-device surface) stays unchanged.
    devices: List<DeviceEntity> = emptyList(),
    // EXP-656: whether our own `devices` cursor is fresh enough for a stale
    // last_seen_at to mean "away" rather than "we haven't heard".
    devicesFresh: Boolean = true,
): List<AgentRow> {
    if (currentUserId == null || teamId == null) return emptyList()
    val issuesById = issues.associateBy { it.id }
    val live = sessions.filter {
        it.userId == currentUserId &&
            it.teamId == teamId &&
            CodingSessionLiveness.isLive(it, nowMs)
    }
    // EXP-535: resolved only while an issueless, actionless in-review batch
    // row actually needs it — an action run merges nothing, and a still
    // running batch has no PR yet (in_review is flipped in the pr_open
    // transaction).
    val batchPrReps = if (live.any { it.isBatchInReview }) {
        openBatchPrRepresentatives(issues, boards, teamId)
    } else {
        emptyList()
    }
    // issueId is null for batch multi-issue sessions — those rows render
    // without an issue link.
    return live.map { session ->
        AgentRow(
            session = session,
            issue = session.issueId?.let(issuesById::get),
            device = resolveSessionDevice(session, devices, nowMs, devicesFresh),
            batchPrIssue = if (session.isBatchInReview) {
                resolveBatchPrIssue(batchPrReps, session.branch)
            } else {
                null
            },
        )
    }
}

/** How many finished rows the DAO pulls before the agent-close-out filter. */
const val RECENT_RUN_QUERY_LIMIT = 50

/** How many finished runs the "Recent runs" list shows. */
const val RECENT_RUN_LIMIT = 10

/**
 * EXP-637: the "Recent runs" list — the caller's OWN finished runs in the
 * SELECTED team that carry the AGENT's close-out (`outcome`, written only by
 * `exponential_sessions_end`, so a killed, merged or swept row without a
 * report never poses as one), newest first by when they ended, capped at
 * [limit]. Keyed on the outcome, not `ended_by` (EXP-673): a person-started
 * run reports first and ends later, with its tab — that end is `client`, and
 * the report must still list. Mirrored on web (`use-agents-data.ts`) and iOS
 * (`RunOutcomePresentation.hasCloseOut`). The DAO already scopes and orders; the rules
 * live here so they are testable and so a wider query can't leak a foreign or
 * still-live row into the list. Signed out or no team selected lists nothing.
 */
fun recentRunRows(
    sessions: List<CodingSessionEntity>,
    issues: List<IssueEntity>,
    currentUserId: String?,
    teamId: String?,
    // EXP-549/550: the synced machine rows, for the byline's live label.
    devices: List<DeviceEntity> = emptyList(),
    nowMs: Long = System.currentTimeMillis(),
    limit: Int = RECENT_RUN_LIMIT,
    // EXP-656: see [agentRows] — an unrefreshed devices cursor renders unknown
    // presence, never offline.
    devicesFresh: Boolean = true,
): List<RecentRunRow> {
    if (currentUserId == null || teamId == null) return emptyList()
    val issuesById = issues.associateBy { it.id }
    // Resolved once for the whole list: a Resume needs the run's OWN machine
    // online and `resume-run`-capable, which only the live device row knows.
    val steerDevices = devices.map { it.toSteerDevice(nowMs, currentUserId) }
    return sessions
        .filter {
            it.userId == currentUserId &&
                it.teamId == teamId &&
                it.status == DomainContract.codingSessionStatusEnded &&
                it.outcome != null
        }
        // ISO-8601 UTC stamps order lexicographically; a row swept before it
        // stamped `ended_at` still sorts off its start time.
        .sortedByDescending { it.endedAt ?: it.startedAt }
        .take(limit)
        .map { session ->
            RecentRunRow(
                session = session,
                issue = session.issueId?.let(issuesById::get),
                device = resolveSessionDevice(session, devices, nowMs, devicesFresh),
                resume = resumeTargetFor(session, steerDevices, currentUserId),
            )
        }
}

// An issueless, actionless in-review session — the only row shape whose merge
// shortcut needs the client-resolved batch PR (EXP-535).
private val CodingSessionEntity.isBatchInReview: Boolean
    get() = issueId == null &&
        actionName == null &&
        status == DomainContract.codingSessionStatusInReview

// The batch launcher's branch namespace (`exp/batch-<id8>`); the contract
// carries no constant for it — matching web's inline literal.
private const val BATCH_BRANCH_PREFIX = "exp/batch-"

/**
 * EXP-535: batch sessions carry no issue linkage, so a batch row resolves its
 * open PR client-side: the team's open-PR issues on an `exp/batch-` branch,
 * collapsed by prUrl to one representative (newest `createdAt`) issue — the
 * Reviews pattern; the server resolves that issue's PR to ALL linked issues
 * on merge. Team scoping goes through live boards ([issues] don't sync
 * team_id).
 */
fun openBatchPrRepresentatives(
    issues: List<IssueEntity>,
    boards: List<BoardEntity>,
    teamId: String?,
): List<IssueEntity> {
    if (teamId == null) return emptyList()
    val teamBoardIds = boards
        .filter { it.teamId == teamId && it.deletedAt == null }
        .mapTo(mutableSetOf()) { it.id }
    val byPrUrl = mutableMapOf<String, IssueEntity>()
    for (issue in issues) {
        val prUrl = issue.prUrl ?: continue
        if (issue.prState != DomainContract.prStateOpen) continue
        if (issue.branch?.startsWith(BATCH_BRANCH_PREFIX) != true) continue
        if (issue.boardId !in teamBoardIds) continue
        val current = byPrUrl[prUrl]
        // ISO-8601 UTC timestamps — lexicographic order IS chronological
        // (same comparison the list sorts already lean on).
        if (current == null || issue.createdAt > current.createdAt) {
            byPrUrl[prUrl] = issue
        }
    }
    return byPrUrl.values.toList()
}

/**
 * EXP-545: a batch session's Merge shortcut must target its OWN PR — the
 * branch the server's pr_open batch flip stamped on the row. Matching "the
 * team's sole open batch PR" alone could offer a teammate's PR once this
 * session's own PR closed unmerged (prState `closed` while the row stays
 * in_review). EXP-546: the pre-EXP-545 branchless rows have drained, so a null
 * [sessionBranch] no longer falls back to "the sole open batch PR" — it
 * resolves nothing, and such a row simply shows no Merge shortcut. Anything
 * ambiguous resolves to null too — with concurrent batch runs Reviews still
 * lists every PR.
 */
fun resolveBatchPrIssue(
    representatives: List<IssueEntity>,
    sessionBranch: String?,
): IssueEntity? =
    sessionBranch?.let { branch -> representatives.filter { it.branch == branch }.singleOrNull() }

/**
 * The synced devices rows → the tab's SteerDevice list (EXP-481): the
 * caller's own machines first, then the SELECTED team's shared servers with
 * their owner resolved from the synced users (a sharing owner is always a
 * member of the sharing team, hence inside the users shape). Each group in
 * [stableDeviceOrder] (EXP-623) — online-by-label first, so heartbeats can't
 * reorder the list. Signed out (null [currentUserId]) lists nothing.
 */
fun composeDeviceList(
    rows: List<DeviceEntity>,
    users: List<UserEntity>,
    teamId: String?,
    currentUserId: String?,
    nowMs: Long,
): List<SteerDevice> {
    if (currentUserId == null) return emptyList()
    val usersById = users.associateBy { it.id }
    val own = rows
        .filter { it.userId == currentUserId }
        .sortedWith(stableDeviceOrder(nowMs))
        .map { it.toSteerDevice(nowMs, currentUserId) }
    val shared = rows
        .filter {
            it.userId != currentUserId &&
                teamId != null &&
                it.sharedTeamId == teamId &&
                it.kind == SteerDevice.KIND_SERVER
        }
        .sortedWith(stableDeviceOrder(nowMs))
        .map { it.toSteerDevice(nowMs, currentUserId, usersById[it.userId]?.name) }
    return own + shared
}

// Terminal issue statuses ineligible to start a new coding run.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
