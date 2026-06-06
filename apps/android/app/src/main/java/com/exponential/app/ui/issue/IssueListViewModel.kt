package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.FilterTab
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.domain.deriveTab
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.matchesFilters
import com.exponential.app.domain.statuses
import com.exponential.app.ui.markdown.removeMarkdownImagesByUrl
import com.exponential.app.ui.markdown.replaceMarkdownImageUrls
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class IssueGroup(val status: IssueStatus, val issues: List<IssueWithLabels>)

data class IssueWithLabels(val issue: IssueEntity, val labels: List<LabelEntity>)

// Intermediate result of the heavy filter/group pipeline. Kept separate from
// IssueListState so the transient UI flags (busy/error/refreshing) can be
// overlaid without rebuilding the grouped list.
private data class GroupedIssueState(
    val project: ProjectEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val filters: IssueFilters = IssueFilters(),
    val tab: FilterTab = FilterTab.All,
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
)

data class IssueListState(
    val project: ProjectEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val filters: IssueFilters = IssueFilters(),
    val tab: FilterTab = FilterTab.All,
    val labels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    val isCreating: Boolean = false,
    val isRefreshing: Boolean = false,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val issueImagesApi: IssueImagesApi,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    private val projectId: String = savedStateHandle["projectId"] ?: ""
    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val _filters = MutableStateFlow(IssueFilters())
    val filters: StateFlow<IssueFilters> = _filters

    private val _busy = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _refreshing = MutableStateFlow(false)
    private val _project = MutableStateFlow<ProjectEntity?>(null)

    // Raw search query is updated on every keystroke (the text field stays
    // instantly responsive via local Compose state), but the expensive
    // filter/group recompute is driven off a debounced snapshot so it runs
    // ~250ms after typing stops — off the keystroke and off the UI thread.
    private val _searchQuery = MutableStateFlow("")
    val searchQuery: StateFlow<String> = _searchQuery
    private val debouncedQuery: Flow<String> = _searchQuery
        .debounce(250)
        .distinctUntilChanged()

    private val labelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else db.labelDao().observeByWorkspace(project.workspaceId)
    }
    private val issueLabelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else db.issueLabelDao().observeByWorkspace(project.workspaceId)
    }
    private val workspaceForProject = _project.flatMapLatest { project ->
        if (project == null) flowOf(null) else db.workspaceDao().observeById(project.workspaceId)
    }
    private val membersForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else db.workspaceMemberDao().observeByWorkspace(project.workspaceId)
    }

    val permissions: StateFlow<WorkspacePermissions> = combine(
        workspaceForProject,
        membersForWorkspace,
        auth.userId,
        auth.isAdmin,
    ) { workspace, members, userId, isAdmin ->
        WorkspacePermissions.resolve(
            workspace = workspace,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && members.any { it.userId == userId },
            memberRole = members.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspacePermissions.Denied)

    // The heavy filter/group/sort pipeline. Recomputes only when one of its
    // *meaningful* data inputs changes (project, issues, labels, joins, filters,
    // users, or the debounced search query). Transient UI flags (busy / error /
    // refreshing) are deliberately kept out so toggling them never rebuilds the
    // grouped list.
    private val groupedState: Flow<GroupedIssueState> = combine(
        listOf(
            _project,
            db.issueDao().observeByProject(projectId),
            labelsForWorkspace,
            issueLabelsForWorkspace,
            _filters,
            db.userDao().observeAll(),
            debouncedQuery,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val project = values[0] as ProjectEntity?
        @Suppress("UNCHECKED_CAST")
        val issues = values[1] as List<IssueEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[2] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val joins = values[3] as List<IssueLabelEntity>
        val filters = values[4] as IssueFilters
        @Suppress("UNCHECKED_CAST")
        val users = values[5] as List<UserEntity>
        val query = values[6] as String

        val joinsByIssue = joins.groupBy { it.issueId }
        val labelsById = labels.associateBy { it.id }
        val trimmedQuery = query.trim()

        val filteredAndDecorated = issues.mapNotNull { issue ->
            val status = IssueStatus.fromWire(issue.status)
            val priority = IssuePriority.fromWire(issue.priority)
            val labelIds = joinsByIssue[issue.id]?.map { it.labelId } ?: emptyList()
            if (!matchesFilters(status, priority, labelIds, filters)) return@mapNotNull null
            if (trimmedQuery.isNotEmpty() && !issue.title.contains(trimmedQuery, ignoreCase = true)) {
                return@mapNotNull null
            }
            val resolvedLabels = labelIds.mapNotNull { labelsById[it] }
            IssueWithLabels(issue, resolvedLabels)
        }

        val grouped = issueStatusOrder.map { st ->
            IssueGroup(
                status = st,
                issues = filteredAndDecorated
                    .filter { IssueStatus.fromWire(it.issue.status) == st }
                    .sortedBy { it.issue.sortOrder },
            )
        }.filter { it.issues.isNotEmpty() }

        GroupedIssueState(
            project = project,
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
            project = grouped.project,
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

    init {
        viewModelScope.launch {
            db.projectDao().observeAll().collect { all ->
                _project.value = all.firstOrNull { it.id == projectId }
            }
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

    fun setSearchQuery(query: String) {
        _searchQuery.value = query
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

    fun updateIssueStatus(issueId: String, status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, status = status.wire))
            }.onFailure { error ->
                _error.value = error.message ?: "Failed to update status"
            }
        }
    }


    fun createIssue(
        title: String,
        status: IssueStatus,
        priority: IssuePriority,
        description: String?,
        dueDate: String?,
        assigneeId: String? = null,
        dueTime: String? = null,
        endTime: String? = null,
        recurrenceInterval: Int? = null,
        recurrenceUnit: String? = null,
        pendingImages: Map<String, android.net.Uri> = emptyMap(),
    ) {
        if (title.isBlank()) return
        viewModelScope.launch {
            _busy.value = true
            _error.value = null
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                val rawDescription = description?.takeIf { it.isNotBlank() }
                val strippedDescription = rawDescription
                    ?.let { removeMarkdownImagesByUrl(it, pendingImages.keys) }
                    ?.takeIf { it.isNotBlank() }

                val created = issuesApi.create(
                    accountId,
                    CreateIssueInput(
                        projectId = projectId,
                        title = title.trim(),
                        status = status.wire,
                        priority = priority.wire,
                        description = strippedDescription,
                        assigneeId = assigneeId,
                        dueDate = dueDate,
                        dueTime = dueTime,
                        endTime = endTime,
                        recurrenceInterval = recurrenceInterval,
                        recurrenceUnit = recurrenceUnit,
                    )
                )

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
                        issuesApi.update(
                            accountId,
                            UpdateIssueInput(id = created.id, description = finalDescription)
                        )
                    }
                }
            } catch (error: Throwable) {
                _error.value = error.message ?: "Failed to create issue"
            } finally {
                _busy.value = false
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
            } catch (_: Throwable) {
                // Skip this image; placeholder will be stripped from final description.
            }
        }
        return out
    }
}
