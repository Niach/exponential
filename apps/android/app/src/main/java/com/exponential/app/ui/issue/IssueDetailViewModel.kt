package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelDao
import com.exponential.app.data.db.LabelDao
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class IssueDetailState(
    val issue: IssueEntity? = null,
    val project: ProjectEntity? = null,
    val workspaceLabels: List<LabelEntity> = emptyList(),
    val issueLabels: List<LabelEntity> = emptyList(),
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class IssueDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val issueDao: IssueDao,
    private val projectDao: ProjectDao,
    private val labelDao: LabelDao,
    private val issueLabelDao: IssueLabelDao,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val issueImagesApi: IssueImagesApi,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    private val issueFlow = issueDao.observeById(issueId)
    private val _project = MutableStateFlow<ProjectEntity?>(null)
    private val workspaceLabelsFlow = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else labelDao.observeByWorkspace(project.workspaceId)
    }

    val state: StateFlow<IssueDetailState> = combine(
        issueFlow,
        _project,
        workspaceLabelsFlow,
        issueLabelDao.observeByIssue(issueId),
    ) { issue, project, allLabels, joins ->
        val labelsById = allLabels.associateBy { it.id }
        IssueDetailState(
            issue = issue,
            project = project,
            workspaceLabels = allLabels,
            issueLabels = joins.mapNotNull { labelsById[it.labelId] },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueDetailState())

    init {
        viewModelScope.launch {
            issueFlow
                .flatMapLatest { issue ->
                    if (issue == null) flowOf(null)
                    else projectDao.observeAll().map { projects ->
                        projects.firstOrNull { it.id == issue.projectId }
                    }
                }
                .collect { _project.value = it }
        }
    }

    fun updateTitle(title: String) {
        if (title.isBlank()) return
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, title = title.trim()))
            }
        }
    }

    fun updateDescription(text: String) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(
                    UpdateIssueInput(id = issueId, description = IssueDescription(text))
                )
            }
        }
    }

    fun updateStatus(status: IssueStatus) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }

    fun updatePriority(priority: IssuePriority) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, priority = priority.wire))
            }
        }
    }

    fun updateDueDate(date: String?) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, dueDate = date))
            }
        }
    }

    fun toggleLabel(labelId: String, isCurrentlyAssigned: Boolean) {
        viewModelScope.launch {
            runCatching {
                if (isCurrentlyAssigned) labelsApi.removeLabel(issueId, labelId)
                else labelsApi.addLabel(issueId, labelId)
            }
        }
    }

    fun createAndAssignLabel(name: String, color: String) {
        val workspaceId = _project.value?.workspaceId ?: return
        viewModelScope.launch {
            runCatching {
                val label = labelsApi.create(CreateLabelInput(workspaceId, name.trim(), color))
                labelsApi.addLabel(issueId, label.id)
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            runCatching { issuesApi.delete(issueId) }.onSuccess { onDeleted() }
        }
    }

    suspend fun uploadImage(uri: android.net.Uri): String? = runCatching {
        val resolver = appContext.contentResolver
        val bytes = resolver.openInputStream(uri)?.use { it.readBytes() }
            ?: return@runCatching null
        val contentType = resolver.getType(uri) ?: "image/jpeg"
        val filename = run {
            resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
            } ?: uri.lastPathSegment ?: "image"
        }
        issueImagesApi.upload(issueId, bytes, filename, contentType).url
    }.getOrNull()
}
