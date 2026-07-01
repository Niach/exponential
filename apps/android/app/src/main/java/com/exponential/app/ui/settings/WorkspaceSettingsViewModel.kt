package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.api.UpdateWorkspaceInput
import com.exponential.app.data.api.WorkspaceInvitesApi
import com.exponential.app.data.api.WorkspaceMembersApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.WorkspaceInviteEntity
import com.exponential.app.data.db.WorkspaceMemberEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class MemberRow(val member: WorkspaceMemberEntity, val user: UserEntity?)

data class WorkspaceSettingsState(
    val workspace: WorkspaceEntity? = null,
    val members: List<MemberRow> = emptyList(),
    val invites: List<WorkspaceInviteEntity> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val projects: List<ProjectEntity> = emptyList(),
    // Server-only repositories registry, loaded over tRPC (never synced).
    val repos: List<WorkspaceRepo> = emptyList(),
    val currentUserId: String? = null,
    val transient: String? = null,
    val createdInviteToken: String? = null,
    val instanceUrl: String? = null,
    val workspaceDeleted: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class WorkspaceSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: WorkspaceSelection,
    private val holder: DatabaseHolder,
    private val membersApi: WorkspaceMembersApi,
    private val invitesApi: WorkspaceInvitesApi,
    private val labelsApi: LabelsApi,
    private val workspacesApi: WorkspacesApi,
    private val repositoriesApi: RepositoriesApi,
) : ViewModel() {

    // Reactive account scoping: a Settings → Workspaces tap on a different
    // server switches the active account and this ViewModel re-scopes to the
    // new account's DB automatically (no rebuild, no pending-handoff flag).
    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val dbAndSelected = combine(dbFlow, selection.selectedId) { db, id -> db to id }

    private val workspaceFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(null)
        else db.workspaceDao().observeAll().map { list -> list.firstOrNull { it.id == id } }
    }

    private val membersFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.workspaceMemberDao().observeByWorkspace(id)
    }
    private val invitesFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.workspaceInviteDao().observeByWorkspace(id)
    }
    private val labelsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.labelDao().observeByWorkspace(id)
    }
    private val projectsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.projectDao().observeByWorkspace(id)
    }

    private val _transient = MutableStateFlow<String?>(null)
    private val _createdInviteToken = MutableStateFlow<String?>(null)
    private val _workspaceDeleted = MutableStateFlow(false)
    private val _repos = MutableStateFlow<List<WorkspaceRepo>>(emptyList())
    val transient: StateFlow<String?> = _transient.asStateFlow()

    init {
        // Repositories aren't an Electric shape — (re)load the registry over
        // tRPC whenever the active account or selected workspace changes.
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { a, w -> a to w }
                .collectLatest { (accountId, workspaceId) ->
                    _repos.value = emptyList()
                    if (accountId != null && workspaceId != null) {
                        runCatching { repositoriesApi.list(accountId, workspaceId) }
                            .onSuccess { _repos.value = it }
                    }
                }
        }
    }

    val state: StateFlow<WorkspaceSettingsState> = combine(
        listOf(
            workspaceFlow,
            membersFlow,
            invitesFlow,
            labelsFlow,
            projectsFlow,
            _repos,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            auth.userId,
            auth.instanceUrl,
            _transient,
            _createdInviteToken,
            _workspaceDeleted,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val workspace = values[0] as WorkspaceEntity?
        @Suppress("UNCHECKED_CAST")
        val members = values[1] as List<WorkspaceMemberEntity>
        @Suppress("UNCHECKED_CAST")
        val invites = values[2] as List<WorkspaceInviteEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[3] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val projects = values[4] as List<ProjectEntity>
        @Suppress("UNCHECKED_CAST")
        val repos = values[5] as List<WorkspaceRepo>
        @Suppress("UNCHECKED_CAST")
        val users = values[6] as List<UserEntity>
        val currentUserId = values[7] as String?
        val instance = values[8] as String?
        val transient = values[9] as String?
        val invite = values[10] as String?
        val deleted = values[11] as Boolean
        WorkspaceSettingsState(
            workspace = workspace,
            members = members.map { m -> MemberRow(m, users.firstOrNull { it.id == m.userId }) },
            invites = invites,
            labels = labels,
            projects = projects,
            repos = repos,
            currentUserId = currentUserId,
            transient = transient,
            createdInviteToken = invite,
            instanceUrl = instance,
            workspaceDeleted = deleted,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspaceSettingsState())

    fun updateRole(memberId: String, role: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.updateRole(accountId, memberId, role) }
            .onFailure { _transient.value = it.message }
    }

    fun removeMember(memberId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.remove(accountId, memberId) }
            .onFailure { _transient.value = it.message }
    }

    fun createInvite(role: String = DomainContract.workspaceRoleMember) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { invitesApi.create(accountId, workspaceId, role) }
            .onSuccess { _createdInviteToken.value = it.token }
            .onFailure { _transient.value = it.message }
    }

    fun revokeInvite(id: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { invitesApi.revoke(accountId, id) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeCreatedInvite() {
        _createdInviteToken.value = null
    }

    fun deleteLabel(labelId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(accountId, workspaceId, labelId) }
            .onFailure { _transient.value = it.message }
    }

    fun renameLabel(labelId: String, name: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(workspaceId, labelId, name = name)) }
            .onFailure { _transient.value = it.message }
    }

    fun recolorLabel(labelId: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(workspaceId, labelId, color = color)) }
            .onFailure { _transient.value = it.message }
    }

    fun createLabel(name: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.create(accountId, CreateLabelInput(workspaceId, name.trim(), color)) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeTransient() { _transient.value = null }

    fun deleteWorkspace() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { workspacesApi.delete(accountId, workspaceId) }
            .onSuccess { _workspaceDeleted.value = true }
            .onFailure { _transient.value = it.message }
    }

    fun deleteProject(projectId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { workspacesApi.deleteProject(accountId, projectId) }
            .onFailure { _transient.value = it.message }
    }

    // --- Repositories registry (server-only; the list is re-fetched after
    // every mutation because there is no Electric shape to sync it back). ---

    fun refreshRepos() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { repositoriesApi.list(accountId, workspaceId) }
            .onSuccess { _repos.value = it }
            .onFailure { _transient.value = it.message }
    }

    private fun repoMutation(block: suspend (accountId: String) -> Unit) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { block(accountId) }
            .onFailure { _transient.value = it.message }
        refreshRepos()
    }

    fun removeRepo(repositoryId: String) =
        repoMutation { repositoriesApi.remove(it, repositoryId) }

    fun linkRepoToProject(projectId: String, repositoryId: String) =
        repoMutation { repositoriesApi.linkProject(it, projectId, repositoryId) }

    fun unlinkRepoFromProject(projectId: String, repositoryId: String) =
        repoMutation { repositoriesApi.unlinkProject(it, projectId, repositoryId) }

    fun setPrimaryRepo(projectId: String, repositoryId: String) =
        repoMutation { repositoriesApi.setPrimary(it, projectId, repositoryId) }

    fun setPublic(isPublic: Boolean) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching {
            workspacesApi.update(
                accountId,
                UpdateWorkspaceInput(
                    id = workspaceId,
                    isPublic = isPublic,
                    // Default policy when first enabling — matches the web flow.
                    publicWritePolicy = if (isPublic) DomainContract.publicWritePolicyMembers else null,
                )
            )
        }.onFailure { _transient.value = it.message }
    }

    fun setPublicWritePolicy(policy: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching {
            workspacesApi.update(accountId, UpdateWorkspaceInput(id = workspaceId, publicWritePolicy = policy))
        }.onFailure { _transient.value = it.message }
    }
}
