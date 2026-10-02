package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.data.electric.elapsedTicker
import com.exponential.app.data.electric.isCatchingUp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueNesting
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.sortIssuesForCategory
import com.exponential.app.ui.steer.onlineStartTargets
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The server caps every bulk procedure's id array at 200; larger selections go
// out as sequential chunks, matching the web bar's BULK_CHUNK_SIZE.
private const val BULK_CHUNK_SIZE = 200

// One group per team status ROW (EXP-314) — the group key is `status.id`
// (row id, or `builtin:<key>` for a constructed fallback).
data class IssueGroup(val status: ResolvedIssueStatus, val issues: List<IssueWithLabels>)

data class IssueWithLabels(
    val issue: IssueEntity,
    val labels: List<LabelEntity>,
    // EXP-980: 0 = a root row, +1 per nesting level ([IssueNesting]). A
    // sub-issue sits in its ROOT's group, wherever its own status is.
    val depth: Int = 0,
    // …which is why the row carries its OWN resolved status: its group's row
    // no longer speaks for it. Null = the caller resolves it (My Issues groups
    // by the cross-team anchor enum).
    val status: ResolvedIssueStatus? = null,
    // EXP-980: the blocks badge numbers, null when there is nothing to show.
    val blocks: IssueGraph.Counts? = null,
)

// Intermediate result of the heavy group/sort pipeline. Kept separate from
// IssueListState so the transient UI flags (busy/error/refreshing) can be
// overlaid without rebuilding the grouped list.
private data class GroupedIssueState(
    val board: BoardEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    val teamUsers: List<UserEntity> = emptyList(),
    val teamStatuses: List<ResolvedIssueStatus> = emptyList(),
)

data class IssueListState(
    val board: BoardEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    // The board team's member users — the assignee-picker + @-mention
    // vocabulary (EXP-487). `users` stays account-wide for avatar display.
    val teamUsers: List<UserEntity> = emptyList(),
    // The board team's statuses in canonical order — the picker
    // vocabulary. Falls back to the constructed builtins until the
    // issue_statuses shape has synced.
    val teamStatuses: List<ResolvedIssueStatus> = emptyList(),
    val isRefreshing: Boolean = false,
    val error: String? = null,
    // EXP-1115: true once the issues shape has reached up-to-date at least
    // once for this account (a zero-row snapshot counts). Until then an
    // empty group list means "still syncing", not "empty board" — the screen
    // shows a spinner instead of the empty state + getting-started cards,
    // which a full resync otherwise flashes while the snapshot lands.
    val issuesSynced: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val steerApi: SteerApi,
    private val stats: SyncStats,
    private val syncManager: SyncManager,
) : ViewModel() {

    // The pushed mount (`board/{boardId}`) seeds the
    // board from the nav args; the Issues tab root has no arg and re-points
    // the ViewModel via setBoard whenever its current-board resolution
    // (last-used → first) changes.
    private val boardIdFlow = MutableStateFlow<String>(savedStateHandle["boardId"] ?: "")

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB snapshot, no key(activeAccountId) rebuild needed).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _error = MutableStateFlow<String?>(null)
    private val _refreshing = MutableStateFlow(false)
    private val _board = MutableStateFlow<BoardEntity?>(null)

    /** Swap the list to another board in place (Issues tab root). */
    fun setBoard(boardId: String) {
        if (boardId == boardIdFlow.value) return
        boardIdFlow.value = boardId
    }

    private val issuesForBoard = combine(dbFlow, boardIdFlow) { db, pid -> db to pid }
        .flatMapLatest { (db, pid) ->
            if (db == null || pid.isBlank()) flowOf(emptyList())
            else db.issueDao().observeByBoard(pid)
        }

    private val labelsForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.labelDao().observeByTeam(board.teamId)
        }
    private val statusesForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.issueStatusDao().observeByTeam(board.teamId)
        }
        .map { rows ->
            // Pre-sync (or a team whose rows haven't arrived) renders the
            // constructed builtin set, so grouping/pickers never go empty.
            if (rows.isEmpty()) IssueStatusResolver.builtinDefaults
            else IssueStatusResolver.teamStatuses(rows)
        }

    private val issueLabelsForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.issueLabelDao().observeByTeam(board.teamId)
        }
    private val teamForBoard = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(null)
            else db.teamDao().observeById(board.teamId)
        }
    private val membersForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.teamMemberDao().observeByTeam(board.teamId)
        }
    // EXP-487: the team's member users — the assignee-picker + @-mention
    // vocabulary. The unscoped `users` list stays for row-avatar display
    // (an ex-member assignee must still render).
    private val usersForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.userDao().observeByTeam(board.teamId)
        }

    // EXP-50: the target team's lone member when it has exactly one — else
    // null. A solo team hides the assignee picker and defaults new issues to
    // that member (the server default-assigns anyway; the UI just stops
    // offering a one-option choice).
    val soloMemberId: StateFlow<String?> = membersForTeam
        .map { members -> members.map { it.userId }.singleOrNull() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val permissions: StateFlow<TeamPermissions> = combine(
        teamForBoard,
        membersForTeam,
        auth.userId,
        auth.isAdmin,
    ) { team, members, userId, isAdmin ->
        TeamPermissions.resolve(
            team = team,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && members.any { it.userId == userId },
            memberRole = members.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamPermissions.Denied)

    // Distinguish "still syncing membership" from "not allowed": when the user
    // is signed in but not yet a resolved member AND the team_members shape
    // hasn't gone live, controls read as pending-sync rather than denied. Drives
    // the "Syncing team…" banner; `stalled` flips the copy when that shape
    // is currently erroring.
    val syncBanner: StateFlow<SyncBanner> = combine(
        permissions,
        auth.activeAccountId,
        stats.state,
    ) { perms, accountId, all ->
        syncBannerFor(perms, all[accountId]?.get(MEMBERS_SHAPE))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SyncBanner.None)

    /**
     * Drives the "Syncing…" chip: the app is behind the server and working on
     * it. Debounced asymmetrically — it only appears after half a second of
     * being behind (a normal poll finishes well inside that, so nothing
     * flickers), and disappears the instant we're caught up.
     */
    val syncingVisible: StateFlow<Boolean> = combine(
        stats.state,
        auth.activeAccountId,
        syncManager.lastKickAt,
        elapsedTicker(),
    ) { all, accountId, lastKick, now ->
        isCatchingUp(all[accountId], lastKick, now)
    }
        .distinctUntilChanged()
        .debounce { visible -> if (visible) 500L else 0L }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /**
     * EXP-980: every synced issue and every synced relation — ONE observation
     * each, shared by the grouping pipeline (nesting + the blocks badges) and
     * by the mini-graph the badge opens. A blocker on another board still
     * counts, so neither may be scoped to this board.
     */
    val allIssues: StateFlow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val relations: StateFlow<List<IssueRelationEntity>> =
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // The heavy group/sort pipeline. Recomputes only when one of its
    // *meaningful* data inputs changes (board, issues, labels, joins or
    // users). Transient UI flags (busy / error / refreshing) are
    // deliberately kept out so toggling them never rebuilds the grouped list.
    // (Issue-list search is gone — cross-board search lives in its own tab.)
    private val groupedState: Flow<GroupedIssueState> = combine(
        listOf(
            _board,
            issuesForBoard,
            labelsForTeam,
            issueLabelsForTeam,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            statusesForTeam,
            usersForTeam,
            // EXP-980: the nesting rows and the blocks badges — computed ONCE
            // per list change here, never per row.
            relations,
            allIssues,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val board = values[0] as BoardEntity?
        @Suppress("UNCHECKED_CAST")
        val issues = values[1] as List<IssueEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[2] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val joins = values[3] as List<IssueLabelEntity>
        @Suppress("UNCHECKED_CAST")
        val users = values[4] as List<UserEntity>
        @Suppress("UNCHECKED_CAST")
        val teamStatuses = values[5] as List<ResolvedIssueStatus>
        @Suppress("UNCHECKED_CAST")
        val teamUsers = values[6] as List<UserEntity>
        @Suppress("UNCHECKED_CAST")
        val relationRows = values[7] as List<IssueRelationEntity>
        @Suppress("UNCHECKED_CAST")
        val syncedIssues = values[8] as List<IssueEntity>

        val joinsByIssue = joins.groupBy { it.issueId }
        val labelsById = labels.associateBy { it.id }

        // status_id → anchor → constructed default (EXP-314); the resolved
        // row's id is the group key.
        val statusByIssue = issues.associate { issue ->
            issue.id to IssueStatusResolver.resolve(issue, teamStatuses)
        }

        val decorated = issues.map { issue ->
            val labelIds = joinsByIssue[issue.id]?.map { it.labelId } ?: emptyList()
            IssueWithLabels(
                issue = issue,
                labels = labelIds.mapNotNull { labelsById[it] },
                status = statusByIssue.getValue(issue.id),
            )
        }

        // One group per team status row, in canonical order; empty groups are
        // hidden. Canonical in-group order (EXP-38) now keys on the row's
        // CATEGORY — see sortIssuesForCategory in domain/IssueDomain.kt.
        val byGroupKey = decorated.groupBy { statusByIssue.getValue(it.issue.id).id }

        fun groupOf(resolved: ResolvedIssueStatus) = IssueGroup(
            status = resolved,
            issues = sortIssuesForCategory(
                category = resolved.category,
                issues = byGroupKey[resolved.id] ?: emptyList(),
            ) { it.issue },
        )

        val knownKeys = teamStatuses.mapTo(mutableSetOf()) { it.id }
        // A resolved status OUTSIDE the team's synced vocabulary (a constructed
        // default while the team's issue_statuses rows are still landing) gets
        // an APPENDED group, in first-encounter order — an issue must never
        // silently vanish from its board. Same contract as web
        // (buildVisibleIssueGroups) and desktop (build_status_groups).
        val extras = decorated
            .map { statusByIssue.getValue(it.issue.id) }
            .filter { it.id !in knownKeys }
            .distinctBy { it.id }

        val grouped = teamStatuses.map(::groupOf).filter { it.issues.isNotEmpty() } +
            extras.map(::groupOf)

        // EXP-980: sub-issues nest under their parent ACROSS the groups — the
        // root decides the group and the position — and every row that has
        // one carries its blocks badge. Both are one pass over the whole list.
        val nested = nestListRows(grouped.map { it.issues }, relationRows, syncedIssues)
        val nestedGroups = grouped.mapIndexedNotNull { index, group ->
            nested[index].takeIf { it.isNotEmpty() }?.let { group.copy(issues = it) }
        }

        GroupedIssueState(
            board = board,
            groups = nestedGroups,
            labels = labels,
            users = users,
            teamUsers = teamUsers,
            teamStatuses = teamStatuses,
        )
    }

    // The issues shape's "up-to-date seen" flag, re-scoped on account switch.
    // Null (no offset row yet — a fresh install or a full resync's wipe)
    // reads as not-yet-synced, same as HomeViewModel's boards flag.
    private val issuesSynced = dbFlow
        .scopedQuery<Boolean?>(null) { db -> db.electricOffsetDao().observeIsLive("issues") }
        .map { it == true }

    val state: StateFlow<IssueListState> = combine(
        groupedState,
        _refreshing,
        _error,
        issuesSynced,
    ) { grouped, refreshing, error, synced ->
        IssueListState(
            board = grouped.board,
            groups = grouped.groups,
            labels = grouped.labels,
            users = grouped.users,
            teamUsers = grouped.teamUsers,
            teamStatuses = grouped.teamStatuses,
            isRefreshing = refreshing,
            error = error,
            issuesSynced = synced,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueListState())

    // ── Multi-select / remote start (EXP-239) ────────────────────────────

    // steer.config is env-derived and static per instance: null = not
    // resolved yet. Fetched lazily by ensureSteerLoaded — most list visits
    // never long-press, so the AgentsViewModel init-time fetch would be waste.
    private val _steerEnabled = MutableStateFlow<Boolean?>(null)
    val steerEnabled: StateFlow<Boolean?> = _steerEnabled

    // The account steer.config was last resolved for — env-derived and static
    // per instance, so a board switch never re-runs it.
    private var steerLoadedForAccount: String? = null

    // The online machines a start can go to: the caller's own plus (EXP-432)
    // the board team's shared servers, off the synced devices shape (EXP-485)
    // — ONLINE only, because the selection bar reads an empty list as "no
    // desktop online". null until ensureSteerLoaded resolves the relay.
    val devices: StateFlow<List<SteerDevice>?> = combine(
        steerDeviceFlow(dbFlow, _board.map { it?.teamId }.distinctUntilChanged(), auth.userId),
        _steerEnabled,
    ) { devices, enabled -> onlineStartTargets(devices, enabled) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    fun ensureSteerLoaded() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            if (steerLoadedForAccount == accountId) return@launch
            steerLoadedForAccount = accountId
            _steerEnabled.value = null
            _steerEnabled.value = runCatching { steerApi.config(accountId).enabled }
                .getOrDefault(false)
        }
    }

    /**
     * Bulk status change from the selection bar (EXP-239). One
     * `issues.bulkUpdate` per chunk: transactional server-side, and past 25
     * ids the server deliberately drops the per-issue notification fan-out.
     * The old per-issue loop bypassed that cap and had no atomicity — a
     * mid-loop failure left a half-applied batch behind one error toast.
     */
    fun bulkUpdateStatus(issueIds: Collection<String>, status: ResolvedIssueStatus) {
        runBulk(issueIds, "Failed to update status") { accountId, ids ->
            issuesApi.bulkUpdate(
                accountId,
                ids,
                status = status.anchorWireOrNull(),
                statusId = status.rowId,
            )
        }
    }

    /** Single-issue status change from an inline list-row tap (EXP-247). */
    fun updateStatus(issueId: String, status: ResolvedIssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(
                    accountId,
                    UpdateIssueInput(
                        id = issueId,
                        status = status.anchorWireOrNull(),
                        statusId = status.rowId,
                    ),
                )
            }.onFailure { error ->
                if (error is CancellationException) throw error
                _error.value = trpcErrorMessage(error, "Failed to update status")
            }
        }
    }

    /** Single-issue priority change from an inline list-row tap (EXP-247). */
    fun updatePriority(issueId: String, priority: IssuePriority) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, priority = priority.wire))
            }.onFailure { error ->
                if (error is CancellationException) throw error
                _error.value = trpcErrorMessage(error, "Failed to update priority")
            }
        }
    }

    /** Bulk priority change from the selection bar (EXP-247). */
    fun bulkUpdatePriority(issueIds: Collection<String>, priority: IssuePriority) {
        runBulk(issueIds, "Failed to update priority") { accountId, ids ->
            issuesApi.bulkUpdate(accountId, ids, priority = priority.wire)
        }
    }

    /** Bulk (re)assignment from the selection bar (EXP-247). null = unassign. */
    fun bulkUpdateAssignee(issueIds: Collection<String>, assigneeId: String?) {
        runBulk(issueIds, "Failed to update assignee") { accountId, ids ->
            // clearAssignee sends an explicit JSON null — without it the
            // shared Json omits the key and "Unassigned" is a silent no-op.
            issuesApi.bulkUpdate(
                accountId,
                ids,
                assigneeId = assigneeId,
                clearAssignee = assigneeId == null,
            )
        }
    }

    /**
     * Bulk label add/remove from the selection bar (EXP-247). [add] true adds
     * the label to each issue, false removes it — the transactional
     * issueLabels bulk procedures, which (unlike the per-issue ones) only
     * record a timeline event for rows they really inserted/deleted.
     */
    fun bulkToggleLabel(issueIds: Collection<String>, labelId: String, add: Boolean) {
        runBulk(issueIds, "Failed to update labels") { accountId, ids ->
            if (add) labelsApi.bulkAddLabel(accountId, ids, labelId)
            else labelsApi.bulkRemoveLabel(accountId, ids, labelId)
        }
    }

    /**
     * Bulk delete from the selection bar (EXP-698 r5). Confirmed on screen
     * first — the server has no undo and the rows vanish from every client at
     * once.
     */
    fun bulkDelete(issueIds: Collection<String>) {
        runBulk(issueIds, "Failed to delete issues") { accountId, ids ->
            issuesApi.bulkDelete(accountId, ids)
        }
    }

    /**
     * Shared driver for the selection-bar writes: chunks the selection at the
     * server's 200-id input cap (same BULK_CHUNK_SIZE as the web bar) and runs
     * the chunks sequentially, surfacing the first failure. Electric replays
     * transactions in commit order, so the rows land in chunk order too.
     */
    private fun runBulk(
        issueIds: Collection<String>,
        errorMessage: String,
        write: suspend (accountId: String, ids: List<String>) -> Unit,
    ) {
        if (issueIds.isEmpty()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            for (chunk in issueIds.toList().chunked(BULK_CHUNK_SIZE)) {
                runCatching { write(accountId, chunk) }.onFailure { error ->
                    if (error is CancellationException) throw error
                    _error.value = trpcErrorMessage(error, errorMessage)
                    return@launch
                }
            }
        }
    }

    init {
        viewModelScope.launch {
            combine(
                dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
                boardIdFlow,
            ) { all, pid ->
                all.firstOrNull { it.id == pid }
            }.collect { _board.value = it }
        }
    }

    /**
     * Triggered by pull-to-refresh: kick every shape loop and hold the spinner
     * until the core shapes have actually polled (or the timeout wins). The
     * gesture used to be a 500ms placebo — on the one occasion it matters, a
     * loop parked in backoff, that was exactly wrong.
     */
    fun refresh() {
        if (_refreshing.value) return
        viewModelScope.launch {
            _refreshing.value = true
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                syncManager.refresh(accountId, timeoutMs = 5_000)
            } finally {
                _refreshing.value = false
            }
        }
    }

}


/**
 * EXP-980: run the shared nesting rule over the WHOLE list at once (the root
 * decides the group, so a sub-issue moves between groups) and stamp each
 * surviving row with its depth and its blocks badge counts.
 *
 * [groups] are the caller's groups in display order; the result has the same
 * shape, with a group a nesting emptied coming back empty (the caller hides
 * it). [syncedIssues] is EVERY synced issue, not just the listed ones — a
 * blocker on another board still counts towards a badge. Shared by the board
 * list and My Issues.
 */
internal fun nestListRows(
    groups: List<List<IssueWithLabels>>,
    relations: List<IssueRelationEntity>,
    syncedIssues: List<IssueEntity>,
): List<List<IssueWithLabels>> {
    val byId = LinkedHashMap<String, IssueWithLabels>()
    for (rows in groups) {
        for (row in rows) byId.putIfAbsent(row.issue.id, row)
    }
    val counts = IssueGraph.blockCounts(relations, syncedIssues)
    val nested = IssueNesting.nestIssueRows(
        groups.map { rows -> rows.map { it.issue.id } },
        relations,
    ) { id -> byId[id]?.issue?.identifier ?: id }
    return nested.map { rows ->
        rows.mapNotNull { row ->
            byId[row.id]?.copy(depth = row.depth, blocks = counts[row.id])
        }
    }
}

/**
 * The enum anchor to write for a status that has NO synced row yet (a
 * constructed `builtin:<key>` fallback) — null once a real row id exists, since
 * the server takes `status` XOR `statusId` (EXP-314).
 */
internal fun ResolvedIssueStatus.anchorWireOrNull(): String? =
    if (rowId != null) null else builtinKey?.wire
