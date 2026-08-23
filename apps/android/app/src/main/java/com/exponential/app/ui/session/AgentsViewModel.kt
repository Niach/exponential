package com.exponential.app.ui.session

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
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.issue.SteerStartState
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
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
// devices.list polling died with the shape; the tRPC list survives only as
// the latestVersions source (one fetch per account).

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

@HiltViewModel
class AgentsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val issuesApi: IssuesApi,
    private val selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's registered machines, online AND offline, from the synced
    // devices shape — plus the selected team's shared servers. null until the
    // shape's initial snapshot has landed (offset is_live), so the section
    // shows nothing rather than a flash of "No machines yet".
    val devices: StateFlow<List<SteerDevice>?> = combine(
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() },
        dbFlow.scopedQuery(null as Boolean?) { it.electricOffsetDao().observeIsLive("devices") },
        combine(selection.selectedId, auth.userId) { teamId, userId -> teamId to userId },
        DeviceLiveness.ticker(),
    ) { rows, users, snapshotLive, (teamId, userId), now ->
        if (snapshotLive != true && rows.isEmpty()) {
            null
        } else {
            composeDeviceList(rows, users, teamId, userId, now)
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

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

    // EXP-549/550: the machine rows every session row joins to — current
    // labels and last_seen_at freshness.
    private val steerEnabledAndDevices = combine(
        _steerEnabled,
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
    ) { steerEnabled, devices -> steerEnabled to devices }

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
    ) { (sessions, issues, boards), (steerEnabled, devices), now, userId, teamId ->
        AgentsState(
            rows = agentRows(sessions, issues, boards, userId, teamId, now, devices),
            steerEnabled = steerEnabled,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

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
                // The rows come from sync now — devices.list survives ONLY as
                // the latestVersions source (informational, one fetch per
                // account; a failure just hides the update hint).
                if (_steerEnabled.value == true) {
                    runCatching { devicesApi.list(accountId) }
                        .onSuccess { _latestVersions.value = it.latestVersions }
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
     * EXP-536: hold a "waiting for the desktop" caption until the run's
     * synced row appears (then hand it to the screen's navigation), or until
     * the deadline passes — a start the desktop REFUSED (conflicted worktree,
     * failed doctor) would otherwise just leave the caption hanging forever,
     * which is exactly the chip-never-disappears bug.
     */
    private suspend fun awaitStartedRun(key: StartedRunKey, device: SteerDevice) {
        val label = device.deviceLabel.ifBlank { device.deviceId }
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
            runCatching { issuesApi.mergePr(accountId, issueId, closeSessions = true) }
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
            device = resolveSessionDevice(session, devices, nowMs),
            batchPrIssue = if (session.isBatchInReview) {
                resolveBatchPrIssue(batchPrReps, session.branch)
            } else {
                null
            },
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
 * in_review). Rows flipped before the stamp existed ([sessionBranch] null)
 * keep the legacy sole-open-PR fallback; anything ambiguous resolves to null
 * — with concurrent batch runs Reviews still lists every PR.
 */
fun resolveBatchPrIssue(
    representatives: List<IssueEntity>,
    sessionBranch: String?,
): IssueEntity? =
    if (sessionBranch != null) {
        representatives.filter { it.branch == sessionBranch }.singleOrNull()
    } else {
        representatives.singleOrNull()
    }

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
