package com.exponential.app.ui.session

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.CodingSessionsApi
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.agentUsageRefreshCommand
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
import com.exponential.app.domain.AgentAccountSection
import com.exponential.app.domain.AgentAccountUsageGroup
import com.exponential.app.domain.AgentAccountsRows
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DeviceFreshness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.resolveMergeTarget
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.resumeTargetFor
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
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
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The Devices tab's model: the caller's machines (EXP-403 registry) plus the
// signed-in user's OWN coding sessions — running (synced coding_sessions shape
// joined to its issue) and finished (EXP-746). EXP-825: the sessions render on
// the Agent page (which reuses this model) and every start goes through the
// composer there, so the remote-start launcher this model used to carry is
// gone; the desktop remains the only session runner.
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
    // EXP-734: what this row's Merge control acts on — an issue (its own, or a
    // batch PR's representative) or, for an action/chat run that opened a PR
    // of its own, the SESSION. Null = nothing to merge.
    val mergeTarget: MergeTarget? = null,
)

/**
 * EXP-746: one FINISHED run in the Devices screen's "Past" list — the session
 * row, its issue when it had one, and the machine that ran it (for the byline
 * and the Resume target).
 */
data class PastRunRow(
    val session: CodingSessionEntity,
    val issue: IssueEntity?,
    val device: SessionDevicePresentation = SessionDevicePresentation.Unknown,
    // Where a Resume would go, or null when the run can't be resumed right
    // now (its machine is gone, offline, or too old to know how).
    val resume: RunResumeTarget? = null,
)

data class AgentsState(
    val rows: List<AgentRow> = emptyList(),
    // steer.config is env-derived and static per instance: null = still
    // loading. Decides whether a row tap opens the live viewer directly or
    // falls back to the issue detail, and whether the devices section shows.
    val steerEnabled: Boolean? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val issuesApi: IssuesApi,
    private val codingSessionsApi: CodingSessionsApi,
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

    // ── EXP-829: the Accounts section (EXP-818's Devices → Accounts) ────────
    // One row per agent ACCOUNT off the synced devices rows — own machines
    // plus the selected team's shared servers — recomputed on the same 30s
    // ticker the machine list's online-ness rides. null until the shape's
    // initial snapshot has landed (the section says "Loading…", never a
    // flash of "nothing reported yet").
    val accountSections: StateFlow<List<AgentAccountSection>?> = combine(
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        dbFlow.scopedQuery(null as Boolean?) { it.electricOffsetDao().observeIsLive("devices") },
        combine(auth.userId, selection.selectedId) { userId, teamId -> userId to teamId },
        DeviceLiveness.ticker(),
    ) { rows, snapshotLive, (userId, teamId), now ->
        if (snapshotLive != true && rows.isEmpty()) null else accountSections(rows, userId, teamId, now)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Refreshes in flight, keyed by account: the usage stamp the account
    // carried when it was queued — the device's re-report MOVES it, and that
    // is what clears the spinner (web `refreshing`, desktop `RefreshMark`).
    private data class RefreshMark(val fetchedAt: String?, val at: Long)

    private val _refreshingAccounts = MutableStateFlow<Map<String, RefreshMark>>(emptyMap())
    val refreshingAccounts: StateFlow<Set<String>> = _refreshingAccounts
        .map { it.keys }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptySet())

    // EXP-817: when the section's OWN refresh last tried each account.
    private val autoAttempts = mutableMapOf<String, Long>()

    // The last failed MANUAL queue attempt, rendered under the header (the
    // desktop's treatment — a snackbar hides behind the bottom nav pill).
    private val _accountsError = MutableStateFlow<String?>(null)
    val accountsError: StateFlow<String?> = _accountsError

    /**
     * Queue `agent_usage_refresh` on the account's refresh target. The answer
     * arrives as a synced `agent_usage` write, never as a command result, so
     * the only local state is the in-flight mark. [silent] is the section's
     * own round: a refusal (a command still queued from the last round is a
     * CONFLICT) is swallowed, the next tick simply looks again.
     */
    fun refreshAccount(group: AgentAccountUsageGroup, silent: Boolean = false) {
        val target = group.refreshTarget ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            if (!silent) _accountsError.value = null
            _refreshingAccounts.value = _refreshingAccounts.value +
                (group.key to RefreshMark(group.usage?.fetchedAt, System.currentTimeMillis()))
            runCatching {
                devicesApi.createCommand(
                    accountId,
                    agentUsageRefreshCommand(target.deviceId, target.agent, target.profileId),
                )
            }.onFailure { t ->
                if (t is CancellationException) throw t
                _refreshingAccounts.value = _refreshingAccounts.value - group.key
                if (!silent) {
                    _accountsError.value =
                        trpcErrorMessage(t, "The refresh could not be queued on the machine.")
                }
            }
        }
    }

    /**
     * EXP-817: keep the section current while it is open — the screen calls
     * this on every sections emission and on a 30s tick (web `useNow`).
     * First the in-flight marks the synced rows have answered (the stamp
     * moved) or that waited past [REFRESH_PENDING_MS] are dropped; then every
     * account with an eligible machine and a freshest report past the floor
     * gets ONE refresh queued — never while one is in flight, never twice
     * inside [AUTO_REFRESH_RETRY_MS]. The floor is the device's own 429
     * budget, so this can never out-poll what the machine allows itself.
     */
    fun autoRefreshAccounts() {
        val groups = accountSections.value?.flatMap { it.groups } ?: return
        val nowMs = System.currentTimeMillis()
        _refreshingAccounts.value = _refreshingAccounts.value.filter { (key, mark) ->
            val group = groups.firstOrNull { it.key == key } ?: return@filter false
            group.usage?.fetchedAt == mark.fetchedAt && nowMs - mark.at <= REFRESH_PENDING_MS
        }
        for (group in groups) {
            if (group.refreshTarget == null || group.key in _refreshingAccounts.value) continue
            if (AgentAccountsRows.refreshAllowedAt(group.usage, nowMs) != null) continue
            val last = autoAttempts[group.key] ?: Long.MIN_VALUE
            if (nowMs - last < AUTO_REFRESH_RETRY_MS) continue
            autoAttempts[group.key] = nowMs
            refreshAccount(group, silent = true)
        }
    }

    // The live rows the list renders from.
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

    // EXP-746: the caller's own FINISHED, person-started sessions in the
    // selected team, newest first — the source of the "Past" list. Queried
    // wider than the list shows so a row the pure filter drops can't push a
    // real one off the end; the DAO already excludes automation runs, which
    // belong to the Automations tab's "Recent automated runs" alone.
    private val endedSessionRows = combine(
        dbFlow,
        selection.selectedId,
        auth.userId,
    ) { db, teamId, userId -> Triple(db, teamId, userId) }
        .flatMapLatest { (db, teamId, userId) ->
            if (db == null || teamId == null || userId == null) {
                flowOf(emptyList())
            } else {
                db.codingSessionDao().observePastByTeamAndUser(
                    teamId = teamId,
                    userId = userId,
                    status = DomainContract.codingSessionStatusEnded,
                    limit = PAST_RUN_QUERY_LIMIT,
                )
            }
        }

    /**
     * EXP-746: the runs that finished, newest first — each expandable to its
     * summary and (on a capable, online machine) a Resume. Empty renders
     * nothing at all.
     */
    val pastRuns: StateFlow<List<PastRunRow>> = combine(
        endedSessionRows,
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        deviceRowsAndFreshness,
        combine(auth.userId, selection.selectedId) { userId, teamId -> userId to teamId },
        DeviceLiveness.ticker(),
    ) { sessions, issues, (devices, polledAt), (userId, teamId), now ->
        pastRunRows(
            sessions, issues, userId, teamId, devices, now,
            devicesFresh = DeviceFreshness.isTrustworthy(polledAt, SystemClock.elapsedRealtime()),
        )
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

    // ── Merge (EXP-498: merging always closes the session) ──────────────────
    // The server merges AND ends the session, so the row drops off this list
    // on its own once the `ended` flip syncs. Keyed by MergeTarget.key
    // (EXP-734: an issue id, or `session:<id>` for a run's own PR): several
    // rows can be in flight at once.
    private val _merging = MutableStateFlow<Set<String>>(emptySet())
    val merging: StateFlow<Set<String>> = _merging

    // Rendered INLINE on the failing row (EXP-323 pattern — a snackbar hides
    // behind the floating bottom nav pill). Cleared by the next attempt.
    private val _mergeErrors = MutableStateFlow<Map<String, MergeFailure>>(emptyMap())
    val mergeErrors: StateFlow<Map<String, MergeFailure>> = _mergeErrors

    /**
     * Squash-merge the row's PR — the server always ends its coding session
     * too (EXP-498). A [MergeTarget.Issue] merges through the issue (for a
     * batch PR the server resolves it to ALL linked issues and completes them
     * together); a [MergeTarget.Session] merges the run's OWN issueless PR
     * (EXP-734), which completes nothing and only closes the run.
     */
    fun merge(target: MergeTarget) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val key = target.key
            _mergeErrors.value = _mergeErrors.value - key
            _merging.value = _merging.value + key
            runCatching {
                when (target) {
                    is MergeTarget.Issue -> issuesApi.mergePr(accountId, target.issueId)
                    is MergeTarget.Session ->
                        codingSessionsApi.mergePr(accountId, target.sessionId)
                }
            }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // common, persistent failures of a squash merge — same copy
                    // as Reviews and the issue Changes tab, and the same
                    // conflict-only gate on the recovery run (EXP-533).
                    _mergeErrors.value = _mergeErrors.value +
                        (key to MergeFailure.from(t, "The pull request could not be merged"))
                }
            _merging.value = _merging.value - key
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
        val issue = session.issueId?.let(issuesById::get)
        val batchPrIssue = if (session.isBatchInReview) {
            resolveBatchPrIssue(batchPrReps, session.branch)
        } else {
            null
        }
        AgentRow(
            session = session,
            issue = issue,
            device = resolveSessionDevice(session, devices, nowMs, devicesFresh),
            batchPrIssue = batchPrIssue,
            // EXP-734: an action or chat run can carry a PR of its OWN (one
            // that links no issue), merged through codingSessions.mergePr.
            mergeTarget = resolveMergeTarget(session, issue, batchPrIssue),
        )
    }
}

/**
 * How long a queued usage refresh shows as in flight before the section gives
 * up on the machine answering (it answers by re-reporting on its next beat).
 * Web `REFRESH_PENDING_MS`.
 */
const val REFRESH_PENDING_MS = 45_000L

/**
 * EXP-817: the section's own refresh never re-tries one account faster than
 * this — a queued command the machine has not answered yet is a CONFLICT on
 * the server, and hammering it buys nothing. Web `AUTO_REFRESH_RETRY_MS`.
 */
const val AUTO_REFRESH_RETRY_MS = 60_000L

/**
 * EXP-829: the Accounts section's rows — the synced devices rows the section
 * reads ([AgentAccountsRows.sectionDevices]) folded into one row per account,
 * attention first, under agent bands in contract order. Online-ness and the
 * refresh eligibility (mine + online + the `agent-usage-refresh` cap) come
 * off the same [SteerDevice] mapping every machine row renders from. Signed
 * out lists nothing.
 */
fun accountSections(
    rows: List<DeviceEntity>,
    currentUserId: String?,
    teamId: String?,
    nowMs: Long,
): List<AgentAccountSection> {
    if (currentUserId == null) return emptyList()
    val devices = AgentAccountsRows.sectionDevices(rows, currentUserId, teamId)
    val capsByDevice = devices.associate { it.deviceId to it.toSteerDevice(nowMs, currentUserId).caps }
    val profileRows = AgentAccountsRows.agentProfileUsageRows(devices, currentUserId) { seen ->
        DeviceLiveness.isOnline(seen, nowMs)
    }
    val groups = AgentAccountsRows.accountUsageGroups(profileRows) { row ->
        AgentAccountsRows.canRefresh(row, capsByDevice[row.deviceId])
    }
    return AgentAccountsRows.sections(AgentAccountsRows.sortAccountGroupsAttentionFirst(groups))
}

/** How many finished rows the DAO pulls before the pure filter narrows them. */
const val PAST_RUN_QUERY_LIMIT = 50

/** How many finished runs the "Past" list shows. Byte-identical ×4
 *  (`PAST_RUN_CAP` on web, iOS `PastRuns.cap`, desktop `PAST_RUNS_CAP`). */
const val PAST_RUN_LIMIT = 20

/**
 * EXP-746: the "Past" list — the caller's OWN finished PERSON-STARTED runs in
 * the SELECTED team, newest first by when they ended, capped at [limit].
 *
 * `started_reason == null` is the whole predicate on top of ownership: a
 * scheduled or event-triggered run belongs to the Automations tab's "Recent
 * automated runs" and must NEVER list here (EXP-676's rule, kept). The DAO
 * already scopes, filters and orders; the rules live here too so they are
 * testable and so a wider query can't leak a foreign, still-live or automated
 * row into the list. Signed out or no team selected lists nothing.
 */
fun pastRunRows(
    sessions: List<CodingSessionEntity>,
    issues: List<IssueEntity>,
    currentUserId: String?,
    teamId: String?,
    // EXP-549/550: the synced machine rows, for the byline's live label.
    devices: List<DeviceEntity> = emptyList(),
    nowMs: Long = System.currentTimeMillis(),
    limit: Int = PAST_RUN_LIMIT,
    // EXP-656: see [agentRows] — an unrefreshed devices cursor renders unknown
    // presence, never offline.
    devicesFresh: Boolean = true,
): List<PastRunRow> {
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
                it.startedReason == null
        }
        // ISO-8601 UTC stamps order lexicographically; a row swept before it
        // stamped `ended_at` still sorts off its heartbeat stamp.
        .sortedByDescending { it.endedAt ?: it.updatedAt }
        .take(limit)
        .map { session ->
            PastRunRow(
                session = session,
                issue = session.issueId?.let(issuesById::get),
                device = resolveSessionDevice(session, devices, nowMs, devicesFresh),
                resume = resumeTargetFor(session, steerDevices, currentUserId),
            )
        }
}

// An issueless, actionless in-review session — the only row shape whose merge
// shortcut needs the client-resolved batch PR (EXP-535). Internal since
// EXP-678: the steer screen's Merge pill resolves its target the same way.
internal val CodingSessionEntity.isBatchInReview: Boolean
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

