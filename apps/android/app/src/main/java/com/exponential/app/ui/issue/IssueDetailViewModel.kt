package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.SubscriptionsApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
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
    val users: List<UserEntity> = emptyList(),
    val assignee: UserEntity? = null,
)

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val subscriptionsApi: SubscriptionsApi,
    private val issueImagesApi: IssueImagesApi,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""
    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val issueFlow = db.issueDao().observeById(issueId)
    private val _project = MutableStateFlow<ProjectEntity?>(null)
    private val workspaceLabelsFlow = _project.flatMapLatest { project ->
        if (project == null) flowOf(emptyList()) else db.labelDao().observeByWorkspace(project.workspaceId)
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

    val state: StateFlow<IssueDetailState> = combine(
        issueFlow,
        _project,
        workspaceLabelsFlow,
        db.issueLabelDao().observeByIssue(issueId),
        db.userDao().observeAll(),
    ) { issue, project, allLabels, joins, users ->
        val labelsById = allLabels.associateBy { it.id }
        IssueDetailState(
            issue = issue,
            project = project,
            workspaceLabels = allLabels,
            issueLabels = joins.mapNotNull { labelsById[it.labelId] },
            users = users,
            assignee = issue?.assigneeId?.let { id -> users.firstOrNull { it.id == id } },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueDetailState())

    // Subscription state (separate StateFlow — the main combine is at the 5-arg
    // typed cap). Drives the Bell/BellOff toggle in the detail top bar.
    val isSubscribed: StateFlow<Boolean> = combine(
        db.issueSubscriberDao().observeByIssue(issueId),
        auth.userId,
    ) { subs, userId ->
        userId != null && subs.any { it.userId == userId && !it.unsubscribed }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    fun toggleSubscribe() {
        val subscribed = isSubscribed.value
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                if (subscribed) subscriptionsApi.unsubscribe(accountId, issueId)
                else subscriptionsApi.subscribe(accountId, issueId)
            }
        }
    }

    // Debounced description autosave: editing fires updateDescription() on every
    // keystroke, but we only hit the API after the user pauses (or on flush),
    // instead of one tRPC mutation per character.
    private val descriptionInput = MutableStateFlow<String?>(null)

    init {
        viewModelScope.launch {
            issueFlow
                .flatMapLatest { issue ->
                    if (issue == null) flowOf(null)
                    else db.projectDao().observeAll().map { projects ->
                        projects.firstOrNull { it.id == issue.projectId }
                    }
                }
                .collect { _project.value = it }
        }
        viewModelScope.launch {
            descriptionInput
                .filterNotNull()
                .debounce(800)
                .collect { saveDescription(it) }
        }
    }

    fun updateTitle(title: String) {
        if (title.isBlank()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, title = title.trim()))
            }
        }
    }

    fun updateDescription(text: String) {
        descriptionInput.value = text
    }

    /** Persist the latest description immediately, e.g. when leaving the screen. */
    fun flushDescription() {
        val text = descriptionInput.value ?: return
        viewModelScope.launch { saveDescription(text) }
    }

    private suspend fun saveDescription(text: String) {
        val accountId = auth.activeAccountId.value ?: return
        // Skip no-op saves (debounce can fire with the already-persisted value).
        if (text == extractDescriptionMarkdown(state.value.issue?.description)) return
        runCatching {
            issuesApi.update(
                accountId,
                UpdateIssueInput(id = issueId, description = IssueDescription(text))
            )
        }
    }

    fun updateStatus(status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }

    fun updatePriority(priority: IssuePriority) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, priority = priority.wire))
            }
        }
    }

    fun updateDueDate(date: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, dueDate = date))
            }
        }
    }

    fun updateAssignee(userId: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, assigneeId = userId))
            }
        }
    }

    fun updateDueTime(time: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, dueTime = time))
            }
        }
    }

    fun updateEndTime(time: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, endTime = time))
            }
        }
    }

    fun updateRecurrence(interval: Int?, unit: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(
                    accountId,
                    UpdateIssueInput(
                        id = issueId,
                        recurrenceInterval = interval,
                        recurrenceUnit = unit,
                    )
                )
            }
        }
    }

    fun toggleLabel(labelId: String, isCurrentlyAssigned: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                if (isCurrentlyAssigned) labelsApi.removeLabel(accountId, issueId, labelId)
                else labelsApi.addLabel(accountId, issueId, labelId)
            }
        }
    }

    fun createAndAssignLabel(name: String, color: String) {
        val workspaceId = _project.value?.workspaceId ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                val label = labelsApi.create(accountId, CreateLabelInput(workspaceId, name.trim(), color))
                labelsApi.addLabel(accountId, issueId, label.id)
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.delete(accountId, issueId) }.onSuccess { onDeleted() }
        }
    }

    // Flips archivedAt between an ISO timestamp (archived) and null (active).
    // The server clamps archivedAt for non-moderators of public workspaces.
    fun toggleArchive() {
        val current = state.value.issue ?: return
        val next: String? = if (current.archivedAt == null) {
            java.time.Instant.now().toString()
        } else null
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, archivedAt = next))
            }
        }
    }

    suspend fun uploadImage(uri: android.net.Uri): String? = runCatching {
        val accountId = auth.activeAccountId.value ?: return@runCatching null
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
        issueImagesApi.upload(accountId, issueId, bytes, filename, contentType).url
    }.getOrNull()
}
