package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FilterTab
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.deriveTab
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.matchesFilters
import com.exponential.app.domain.sortIssuesForGroup
import com.exponential.app.domain.statuses
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.removeMarkdownImagesByUrl
import com.exponential.app.ui.markdown.replaceMarkdownImageUrls
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class IssueGroup(val status: IssueStatus, val issues: List<IssueWithLabels>)

data class IssueWithLabels(val issue: IssueEntity, val labels: List<LabelEntity>)

// Intermediate result of the heavy filter/group pipeline. Kept separate from
// IssueListState so the transient UI flags (busy/error/refreshing) can be
// overlaid without rebuilding the grouped list.
private data class GroupedIssueState(
    val board: BoardEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val filters: IssueFilters = IssueFilters(),
    val tab: FilterTab = FilterTab.All,
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
)

data class IssueListState(
    val board: BoardEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val filters: IssueFilters = IssueFilters(),
    val tab: FilterTab = FilterTab.All,
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    val isCreating: Boolean = false,
    val isRefreshing: Boolean = false,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class IssueListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val issueImagesApi: IssueImagesApi,
    private val steerApi: SteerApi,
    private val stats: SyncStats,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    // Pushed mounts (`board/{boardId}`, `board/{boardId}/new`) seed the
    // board from the nav args; the Issues tab root has no arg and re-points
    // the ViewModel via setBoard whenever its current-board resolution
    // (last-used → first) changes.
    private val boardIdFlow = MutableStateFlow<String>(savedStateHandle["boardId"] ?: "")

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB snapshot, no key(activeAccountId) rebuild needed).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _filters = MutableStateFlow(IssueFilters())
    val filters: StateFlow<IssueFilters> = _filters

    private val _busy = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _refreshing = MutableStateFlow(false)
    private val _board = MutableStateFlow<BoardEntity?>(null)

    /** Swap the list to another board in place (Issues tab root). */
    fun setBoard(boardId: String) {
        if (boardId == boardIdFlow.value) return
        // Filters can reference another team's labels — start clean.
        _filters.value = IssueFilters()
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
     * The target board's team issues, newest-first — drives the create
     * screen's `#IDENTIFIER` autocomplete (masterplan §5e). Same scoping as
     * IssueDetailViewModel.issueRefCandidates / the web IssueRefProvider.
     */
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        _board,
    ) { issues, boards, board ->
        if (board == null) {
            emptyList()
        } else {
            val teamBoardIds = boards
                .filter { it.teamId == board.teamId }
                .map { it.id }
                .toSet()
            issues
                .filter { it.boardId in teamBoardIds }
                .sortedByDescending { it.createdAt }
                .map { IssueRefTarget(it.id, it.identifier, it.title) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // The heavy filter/group/sort pipeline. Recomputes only when one of its
    // *meaningful* data inputs changes (board, issues, labels, joins,
    // filters, or users). Transient UI flags (busy / error / refreshing) are
    // deliberately kept out so toggling them never rebuilds the grouped list.
    // (Issue-list search is gone — cross-board search lives in its own tab.)
    private val groupedState: Flow<GroupedIssueState> = combine(
        listOf(
            _board,
            issuesForBoard,
            labelsForTeam,
            issueLabelsForTeam,
            _filters,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
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
        val filters = values[4] as IssueFilters
        @Suppress("UNCHECKED_CAST")
        val users = values[5] as List<UserEntity>

        val joinsByIssue = joins.groupBy { it.issueId }
        val labelsById = labels.associateBy { it.id }

        val filteredAndDecorated = issues.mapNotNull { issue ->
            val status = IssueStatus.fromWire(issue.status)
            val priority = IssuePriority.fromWire(issue.priority)
            val labelIds = joinsByIssue[issue.id]?.map { it.labelId } ?: emptyList()
            if (!matchesFilters(status, priority, labelIds, filters)) return@mapNotNull null
            val resolvedLabels = labelIds.mapNotNull { labelsById[it] }
            IssueWithLabels(issue, resolvedLabels)
        }

        // Canonical in-group order (EXP-38) — shared with MyIssues and the
        // other clients; see sortIssuesForGroup in domain/IssueDomain.kt.
        val grouped = issueStatusOrder.map { st ->
            IssueGroup(
                status = st,
                issues = sortIssuesForGroup(
                    status = st,
                    issues = filteredAndDecorated.filter { IssueStatus.fromWire(it.issue.status) == st },
                ) { it.issue },
            )
        }.filter { it.issues.isNotEmpty() }

        GroupedIssueState(
            board = board,
            groups = grouped,
            filters = filters,
            tab = deriveTab(filters.statuses),
            labels = labels,
            users = users,
        )
    }

    val state: StateFlow<IssueListState> = combine(
        groupedState,
        _busy,
        _refreshing,
        _error,
    ) { grouped, busy, refreshing, error ->
        IssueListState(
            board = grouped.board,
            groups = grouped.groups,
            filters = grouped.filters,
            tab = grouped.tab,
            labels = grouped.labels,
            users = grouped.users,
            isCreating = busy,
            isRefreshing = refreshing,
            error = error,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueListState())

    // ── Multi-select / remote start (EXP-239) ────────────────────────────

    // steer.config is env-derived and static per instance: null = not
    // resolved yet. Fetched lazily by ensureSteerLoaded — most list visits
    // never long-press, so the AgentsViewModel init-time fetch would be waste.
    private val _steerEnabled = MutableStateFlow<Boolean?>(null)
    val steerEnabled: StateFlow<Boolean?> = _steerEnabled

    // The caller's online desktops (relay presence). null = not loaded yet.
    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    val devices: StateFlow<List<SteerDevice>?> = _devices

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    private var steerLoadedForAccount: String? = null

    /**
     * Resolve relay availability + device presence when selection mode starts
     * on a repo-backed board, so the bar's Start coding is ready by the time
     * it's tapped. Once per account (an account switch re-resolves).
     */
    fun ensureSteerLoaded() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            if (steerLoadedForAccount == accountId) return@launch
            steerLoadedForAccount = accountId
            _steerEnabled.value = null
            _devices.value = null
            val enabled = runCatching { steerApi.config(accountId).enabled }
                .getOrDefault(false)
            _steerEnabled.value = enabled
            _devices.value = if (enabled) {
                runCatching { steerApi.myDevices(accountId).devices }.getOrDefault(emptyList())
            } else {
                emptyList()
            }
        }
    }

    /**
     * Issues the selection bar's Start-coding sheet can queue: this board's
     * eligible issues — repo-backed live board only, non-archived,
     * non-terminal, not merged — `updatedAt` desc. Mirrors
     * AgentsViewModel.startCandidates but board-scoped (the bar lives on one
     * board, which also guarantees the one-repository-per-run rule). Built
     * from the RAW board issues, not the filtered groups, so the sheet's own
     * search can reach issues the active tab preset hides.
     */
    val startCandidates: StateFlow<List<StartIssueOption>> = combine(
        issuesForBoard,
        _board,
    ) { issues, board ->
        val repoId = board?.repositoryId
        if (board == null || repoId == null || board.archivedAt != null || board.deletedAt != null) {
            emptyList()
        } else {
            issues
                .filter {
                    it.archivedAt == null &&
                        it.status !in TERMINAL_ISSUE_STATUSES &&
                        it.prState != DomainContract.prStateMerged
                }
                .sortedByDescending { it.updatedAt }
                .map { issue ->
                    StartIssueOption(
                        id = issue.id,
                        identifier = issue.identifier,
                        title = issue.title,
                        repositoryId = repoId,
                        status = issue.status,
                        priority = issue.priority,
                    )
                }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Remote-start on a picked desktop (AgentsViewModel.startCoding's twin):
     * [issueIds] of size 1 launches a plain single session, 2+ a batch. Sent
     * state re-enables after a grace window in case the desktop never picks
     * up (the coding_sessions row would otherwise surface via Electric).
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        if (issueIds.isEmpty()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val isBatch = issueIds.size >= 2
            _startState.value = SteerStartState.Sending
            try {
                if (isBatch) {
                    steerApi.startSession(accountId, issueIds, device.deviceId, options)
                } else {
                    steerApi.startSession(accountId, issueIds.first(), device.deviceId, options)
                }
                _startState.value = SteerStartState.Sent(device.deviceLabel, isBatch)
                delay(30_000)
                if (_startState.value is SteerStartState.Sent) {
                    _startState.value = SteerStartState.Idle
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /** Tap-to-dismiss for a lingering Failed chip (Sent auto-clears). */
    fun dismissStartState() {
        if (_startState.value is SteerStartState.Failed) {
            _startState.value = SteerStartState.Idle
        }
    }

    /**
     * Bulk status change from the selection bar (EXP-239). Sequential — bar
     * selections are small and each update is an independent server write.
     */
    fun bulkUpdateStatus(issueIds: Collection<String>, status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            for (id in issueIds) {
                runCatching {
                    issuesApi.update(accountId, UpdateIssueInput(id = id, status = status.wire))
                }.onFailure { error ->
                    if (error is CancellationException) throw error
                    _error.value = error.message ?: "Failed to update status"
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

    fun setTab(tab: FilterTab) {
        _filters.value = _filters.value.copy(statuses = tab.statuses())
    }

    fun setFilters(filters: IssueFilters) {
        _filters.value = filters
    }

    fun toggleStatus(status: IssueStatus) {
        val next = _filters.value.statuses.toMutableSet().apply { if (!add(status)) remove(status) }
        _filters.value = _filters.value.copy(statuses = next)
    }

    fun togglePriority(priority: IssuePriority) {
        val next = _filters.value.priorities.toMutableSet().apply { if (!add(priority)) remove(priority) }
        _filters.value = _filters.value.copy(priorities = next)
    }

    fun toggleLabel(labelId: String) {
        val next = _filters.value.labelIds.toMutableSet().apply { if (!add(labelId)) remove(labelId) }
        _filters.value = _filters.value.copy(labelIds = next)
    }

    fun clearFilters() {
        _filters.value = IssueFilters()
    }

    /**
     * Triggered by pull-to-refresh. Data is already live via Electric + Room,
     * so this is just a short spinner so the gesture feels acknowledged.
     */
    fun refresh() {
        if (_refreshing.value) return
        viewModelScope.launch {
            _refreshing.value = true
            try {
                delay(500)
            } finally {
                _refreshing.value = false
            }
        }
    }

    /**
     * Create a team label from the create screen's picker sheet and return
     * it (so the caller can pre-select it on the issue being drafted), or null
     * on failure. Suspends — the create screen awaits it on its own scope.
     */
    suspend fun createLabel(name: String, color: String): LabelEntity? {
        val teamId = _board.value?.teamId ?: return null
        val accountId = auth.activeAccountId.value ?: return null
        return runCatching {
            labelsApi.create(accountId, CreateLabelInput(teamId, name.trim(), color))
        }.onSuccess { created ->
            // Optimistic local upsert so the label appears immediately instead of
            // waiting for the labels shape's next poll (idempotent REPLACE;
            // Electric re-delivers the same row, so this is only a head-start).
            runCatching { holder.database(forAccountId = accountId).labelDao().upsert(created) }
        }.onFailure { error ->
            _error.value = error.message ?: "Failed to create label"
        }.getOrNull()
    }


    // Suspends until the issue (and any image upload/patch) is committed, then
    // returns success. The caller awaits this before navigating away — the create
    // screen has its own ViewModel scope, so a fire-and-forget launch would be
    // cancelled the moment the screen pops, dropping the write.
    suspend fun createIssueAwait(
        title: String,
        status: IssueStatus,
        priority: IssuePriority,
        description: String?,
        dueDate: String?,
        assigneeId: String? = null,
        dueTime: String? = null,
        endTime: String? = null,
        labelIds: List<String> = emptyList(),
        pendingImages: Map<String, android.net.Uri> = emptyMap(),
    ): Boolean {
        if (title.isBlank()) return false
        _busy.value = true
        _error.value = null
        return try {
            val accountId = auth.activeAccountId.value ?: return false
            val rawDescription = description?.takeIf { it.isNotBlank() }
            val strippedDescription = rawDescription
                ?.let { removeMarkdownImagesByUrl(it, pendingImages.keys) }
                ?.takeIf { it.isNotBlank() }

            val created = issuesApi.create(
                accountId,
                CreateIssueInput(
                    boardId = boardIdFlow.value,
                    title = title.trim(),
                    status = status.wire,
                    priority = priority.wire,
                    description = strippedDescription,
                    assigneeId = assigneeId,
                    dueDate = dueDate,
                    dueTime = dueTime,
                    endTime = endTime,
                    labelIds = labelIds.takeIf { it.isNotEmpty() },
                )
            )
            upsertCreatedLocally(accountId, created, labelIds)

            if (rawDescription != null && pendingImages.isNotEmpty()) {
                val urlByPlaceholder = uploadPendingImages(accountId, created.id, pendingImages)
                val finalDescription = replaceMarkdownImageUrls(
                    markdown = removeMarkdownImagesByUrl(
                        rawDescription,
                        pendingImages.keys.minus(urlByPlaceholder.keys),
                    ),
                    replacements = urlByPlaceholder,
                )
                if (finalDescription != strippedDescription.orEmpty() && finalDescription.isNotBlank()) {
                    val updated = issuesApi.update(
                        accountId,
                        UpdateIssueInput(id = created.id, description = finalDescription)
                    )
                    runCatching {
                        holder.database(forAccountId = accountId).issueDao().upsert(updated)
                    }
                }
            }
            true
        } catch (error: Throwable) {
            _error.value = error.message ?: "Failed to create issue"
            false
        } finally {
            _busy.value = false
        }
    }

    /**
     * Mirror a freshly-created issue (and its label joins) into local Room
     * immediately, instead of waiting for the Electric long-poll to deliver it.
     * The share-intent path cold-starts the process, so its `issues` shape is
     * usually still finishing its initial/catch-up sync when the issue is
     * created and won't surface the new row until the next live poll (up to
     * ~60s — the reported "shows up after a minute"). Electric re-delivers the
     * same rows on its next cycle; every upsert here is idempotent (REPLACE),
     * so this is purely a visibility head-start (EXP-19). Best-effort: a DB
     * hiccup here must not fail the already-committed server create.
     */
    private suspend fun upsertCreatedLocally(
        accountId: String,
        issue: IssueEntity,
        labelIds: List<String>,
    ) {
        runCatching {
            val db = holder.database(forAccountId = accountId)
            db.issueDao().upsert(issue)
            if (labelIds.isNotEmpty()) {
                // issue_labels carries a denormalized team_id (Electric
                // shape scoping). Resolve it from the board; skip the joins if
                // we can't (Electric still delivers them on its next poll).
                val teamId = db.boardDao().getActiveById(issue.boardId)?.teamId
                    ?: _board.value?.teamId
                if (teamId != null) {
                    for (labelId in labelIds) {
                        db.issueLabelDao().upsert(
                            IssueLabelEntity(
                                issueId = issue.id,
                                labelId = labelId,
                                teamId = teamId,
                            )
                        )
                    }
                }
            }
        }
    }

    private suspend fun uploadPendingImages(
        accountId: String,
        issueId: String,
        pending: Map<String, android.net.Uri>,
    ): Map<String, String> {
        val out = mutableMapOf<String, String>()
        val resolver = appContext.contentResolver
        for ((placeholder, uri) in pending) {
            try {
                val bytes = resolver.openInputStream(uri)?.use { it.readBytes() } ?: continue
                val contentType = resolver.getType(uri) ?: "image/jpeg"
                val filename = run {
                    resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                        val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                        if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
                    } ?: uri.lastPathSegment ?: "image"
                }
                val uploaded = issueImagesApi.upload(accountId, issueId, bytes, filename, contentType)
                out[placeholder] = uploaded.url
            } catch (cancel: kotlinx.coroutines.CancellationException) {
                throw cancel
            } catch (error: Throwable) {
                // Skip this image; placeholder will be stripped from final
                // description. Log the actual server rejection — silent drops
                // made upload failures undiagnosable (EXP-61).
                android.util.Log.w("IssueListViewModel", "Pending image upload failed", error)
            }
        }
        return out
    }
}

// Terminal issue statuses ineligible to start a new coding run (same set as
// AgentsViewModel / the iOS candidates builders).
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
