package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issueStatusOrder
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class IssueGroup(val status: IssueStatus, val issues: List<IssueEntity>)

data class IssueListState(
    val project: ProjectEntity? = null,
    val groups: List<IssueGroup> = emptyList(),
    val isCreating: Boolean = false,
    val error: String? = null,
)

@HiltViewModel
class IssueListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val projectDao: ProjectDao,
    private val issueDao: IssueDao,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    private val projectId: String = savedStateHandle["projectId"] ?: ""

    private val projectFlow = kotlinx.coroutines.flow.flow {
        emit(projectDao.observeAll())
    }

    private val _project = MutableStateFlow<ProjectEntity?>(null)
    private val _busy = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)

    val state: StateFlow<IssueListState> = combine(
        _project,
        issueDao.observeByProject(projectId),
        _busy,
        _error,
    ) { project, issues, busy, error ->
        val grouped = issueStatusOrder.map { status ->
            IssueGroup(
                status = status,
                issues = issues
                    .filter { IssueStatus.fromWire(it.status) == status }
                    .sortedBy { it.sortOrder },
            )
        }.filter { it.issues.isNotEmpty() }
        IssueListState(project = project, groups = grouped, isCreating = busy, error = error)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueListState())

    init {
        viewModelScope.launch {
            projectDao.observeAll().collect { all ->
                _project.value = all.firstOrNull { it.id == projectId }
            }
        }
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
