package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelDao
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelDao
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.domain.FilterTab
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.deriveTab
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.matchesFilters
import com.exponential.app.domain.statuses
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
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
    val isCreating: Boolean = false,
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
    private val issuesApi: IssuesApi,
) : ViewModel() {

    private val projectId: String = savedStateHandle["projectId"] ?: ""

    private val _filters = MutableStateFlow(IssueFilters())
    val filters: StateFlow<IssueFilters> = _filters

    private val _busy = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _project = MutableStateFlow<ProjectEntity?>(null)

    private val labelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else labelDao.observeByWorkspace(project.workspaceId)
    }
    private val issueLabelsForWorkspace = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else issueLabelDao.observeByWorkspace(project.workspaceId)
    }

    val state: StateFlow<IssueListState> = combine(
        listOf(
            _project,
            issueDao.observeByProject(projectId),
            labelsForWorkspace,
            issueLabelsForWorkspace,
            _filters,
            _busy,
            _error,
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
            isCreating = busy,
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

    fun clearFilters() {
        _filters.value = IssueFilters()
    }

    fun createIssue(
        title: String,
        status: IssueStatus,
        priority: IssuePriority,
        description: String?,
        dueDate: String?,
    ) {
        if (title.isBlank()) return
        viewModelScope.launch {
            _busy.value = true
            _error.value = null
            try {
                issuesApi.create(
                    CreateIssueInput(
                        projectId = projectId,
                        title = title.trim(),
                        status = status.wire,
                        priority = priority.wire,
                        description = description?.takeIf { it.isNotBlank() }?.let { IssueDescription(it) },
                        dueDate = dueDate,
                    )
                )
            } catch (error: Throwable) {
                _error.value = error.message ?: "Failed to create issue"
            } finally {
                _busy.value = false
            }
        }
    }
}
