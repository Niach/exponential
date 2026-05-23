package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelDao
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelDao
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.UserDao
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.WorkspaceDao
import com.exponential.app.data.db.WorkspaceMemberDao
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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class IssueGroup(val status: IssueStatus, val issues: List<IssueWithLabels>)

data class IssueWithLabels(val issue: IssueEntity, val labels: List<LabelEntity>)

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

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class IssueListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val projectDao: ProjectDao,
    private val issueDao: IssueDao,
    private val issueLabelDao: IssueLabelDao,
    private val labelDao: LabelDao,
    private val userDao: UserDao,
    private val workspaceDao: WorkspaceDao,
    private val workspaceMemberDao: WorkspaceMemberDao,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val issueImagesApi: IssueImagesApi,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    private val projectId: String = savedStateHandle["projectId"] ?: ""

    private val _filters = MutableStateFlow(IssueFilters())
    val filters: StateFlow<IssueFilters> = _filters

    private val _busy = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _refreshing = MutableStateFlow(false)
    private val _project = MutableStateFlow<ProjectEntity?>(null)

    private val labelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else labelDao.observeByWorkspace(project.workspaceId)
    }
    private val issueLabelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else issueLabelDao.observeByWorkspace(project.workspaceId)
    }
    private val workspaceForProject = _project.flatMapLatest { project ->
        if (project == null) flowOf(null) else workspaceDao.observeById(project.workspaceId)
    }
    private val membersForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else workspaceMemberDao.observeByWorkspace(project.workspaceId)
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

    val state: StateFlow<IssueListState> = combine(
        listOf(
            _project,
            issueDao.observeByProject(projectId),
            labelsForWorkspace,
            issueLabelsForWorkspace,
            _filters,
            _busy,
            _error,
            userDao.observeAll(),
            _refreshing,
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
        val busy = values[5] as Boolean
        val error = values[6] as String?
        @Suppress("UNCHECKED_CAST")
        val users = values[7] as List<UserEntity>
        val refreshing = values[8] as Boolean

        val joinsByIssue = joins.groupBy { it.issueId }
        val labelsById = labels.associateBy { it.id }

        val filteredAndDecorated = issues.mapNotNull { issue ->
            val status = IssueStatus.fromWire(issue.status)
            val priority = IssuePriority.fromWire(issue.priority)
            val labelIds = joinsByIssue[issue.id]?.map { it.labelId } ?: emptyList()
            if (!matchesFilters(status, priority, labelIds, issue.assigneeId, filters)) return@mapNotNull null
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

        IssueListState(
            project = project,
            groups = grouped,
            filters = filters,
            tab = deriveTab(filters.statuses),
            labels = labels,
            users = users,
            isCreating = busy,
            isRefreshing = refreshing,
            error = error,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueListState())

    init {
        viewModelScope.launch {
            projectDao.observeAll().collect { all ->
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

    fun toggleAssignee(userId: String) {
        val next = _filters.value.assigneeIds.toMutableSet().apply { if (!add(userId)) remove(userId) }
        _filters.value = _filters.value.copy(assigneeIds = next)
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

    fun updateIssueStatus(issueId: String, status: IssueStatus) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, status = status.wire))
            }.onFailure { error ->
                _error.value = error.message ?: "Failed to update status"
            }
        }
    }

    fun archiveIssue(issueId: String) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(
                    UpdateIssueInput(
                        id = issueId,
                        archivedAt = java.time.Instant.now().toString(),
                    )
                )
            }.onFailure { error ->
                _error.value = error.message ?: "Failed to archive issue"
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
                val rawDescription = description?.takeIf { it.isNotBlank() }
                val strippedDescription = rawDescription
                    ?.let { removeMarkdownImagesByUrl(it, pendingImages.keys) }
                    ?.takeIf { it.isNotBlank() }

                val created = issuesApi.create(
                    CreateIssueInput(
                        projectId = projectId,
                        title = title.trim(),
                        status = status.wire,
                        priority = priority.wire,
                        description = strippedDescription?.let { IssueDescription(it) },
                        assigneeId = assigneeId,
                        dueDate = dueDate,
                        dueTime = dueTime,
                        endTime = endTime,
                        recurrenceInterval = recurrenceInterval,
                        recurrenceUnit = recurrenceUnit,
                    )
                )

                if (rawDescription != null && pendingImages.isNotEmpty()) {
                    val urlByPlaceholder = uploadPendingImages(created.id, pendingImages)
                    val finalDescription = replaceMarkdownImageUrls(
                        markdown = removeMarkdownImagesByUrl(
                            rawDescription,
                            pendingImages.keys.minus(urlByPlaceholder.keys),
                        ),
                        replacements = urlByPlaceholder,
                    )
                    if (finalDescription != strippedDescription.orEmpty() && finalDescription.isNotBlank()) {
                        issuesApi.update(
                            UpdateIssueInput(id = created.id, description = IssueDescription(finalDescription))
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
                val uploaded = issueImagesApi.upload(issueId, bytes, filename, contentType)
                out[placeholder] = uploaded.url
            } catch (_: Throwable) {
                // Skip this image; placeholder will be stripped from final description.
            }
        }
        return out
    }
}
