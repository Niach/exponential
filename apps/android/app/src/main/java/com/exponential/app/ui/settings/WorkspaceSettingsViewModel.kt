package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.api.WorkspaceInvitesApi
import com.exponential.app.data.api.WorkspaceMembersApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
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
    // GitHub grant state (integrations.github.repos, mobile-marked URLs) —
    // drives the "Reconnect GitHub" affordance when a linked installation has
    // no captured grants (needsReauth). Null while loading / not configured.
    val github: GithubReposResult? = null,
    val currentUserId: String? = null,
    val transient: String? = null,
    val createdInviteToken: String? = null,
    val instanceUrl: String? = null,
    val workspaceDeleted: Boolean = false,
    // All synced workspaces of the account (membership-scoped shape), for the
    // only-workspace derivation below.
    val allWorkspaces: List<WorkspaceEntity> = emptyList(),
) {
    // Owner-gated controls key off this (hidden for non-owners — web parity).
    val isOwner: Boolean
        get() = currentUserId != null && members.any {
            it.member.userId == currentUserId && it.member.role == DomainContract.workspaceRoleOwner
        }

    // Synced workspaces minus the bootstrap feedback workspace == "my personal
    // workspaces". Deleting the last one is server-refused (EXP-82);
    // empty-while-loading biases the delete affordance to disabled.
    val isOnlyWorkspace: Boolean
        get() = allWorkspaces.count { it.slug != "feedback" } <= 1
}

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
    private val integrationsApi: IntegrationsApi,
    private val deepLinkBus: DeepLinkBus,
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
    private val _github = MutableStateFlow<GithubReposResult?>(null)
    val transient: StateFlow<String?> = _transient.asStateFlow()

    init {
        // Repositories aren't an Electric shape — (re)load the registry over
        // tRPC whenever the active account or selected workspace changes. The
        // GitHub grant state rides along (needsReauth drives the reconnect row).
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { a, w -> a to w }
                .collectLatest { (accountId, workspaceId) ->
                    _repos.value = emptyList()
                    _github.value = null
                    if (accountId != null && workspaceId != null) {
                        runCatching { repositoriesApi.list(accountId, workspaceId) }
                            .onSuccess { _repos.value = it }
                        runCatching { integrationsApi.githubRepos(accountId, workspaceId) }
                            .onSuccess { _github.value = it }
                    }
                }
        }
        // The reconnect Custom Tab ends on the server's "connected" page, which
        // fires exponential://github-connected — re-fetch so the needsReauth row clears
        // without leaving the screen (mirrors GithubRepoPickerViewModel).
        viewModelScope.launch {
            deepLinkBus.target.collect { target ->
                if (target is DeepLinkBus.Target.GithubConnected) {
                    deepLinkBus.consume()
                    refreshGithub()
                }
            }
        }
    }

    // Re-fetch the registry + grant state (bypassing the server's repo cache)
    // after a GitHub reconnect lands.
    private fun refreshGithub() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { repositoriesApi.list(accountId, workspaceId) }
            .onSuccess { _repos.value = it }
        runCatching { integrationsApi.githubRepos(accountId, workspaceId, refresh = true) }
            .onSuccess { _github.value = it }
    }

    val state: StateFlow<WorkspaceSettingsState> = combine(
        listOf(
            workspaceFlow,
            membersFlow,
            invitesFlow,
            labelsFlow,
            projectsFlow,
            _repos,
            _github,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            auth.userId,
            auth.instanceUrl,
            _transient,
            _createdInviteToken,
            _workspaceDeleted,
            dbFlow.scopedQuery(emptyList<WorkspaceEntity>()) { it.workspaceDao().observeAll() },
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
        val github = values[6] as GithubReposResult?
        @Suppress("UNCHECKED_CAST")
        val users = values[7] as List<UserEntity>
        val currentUserId = values[8] as String?
        val instance = values[9] as String?
        val transient = values[10] as String?
        val invite = values[11] as String?
        val deleted = values[12] as Boolean
        @Suppress("UNCHECKED_CAST")
        val allWorkspaces = values[13] as List<WorkspaceEntity>
        WorkspaceSettingsState(
            workspace = workspace,
            // Synthetic agent users (widget reporters etc.) are workspace
            // members server-side but never shown in the roster — iOS hides them
            // too. Rows whose user hasn't synced yet (user == null) still render
            // (userDisplayName degrades to a "Member <id>" placeholder).
            members = members
                .map { m -> MemberRow(m, users.firstOrNull { it.id == m.userId }) }
                .filter { it.user?.isAgent != true },
            invites = invites,
            labels = labels,
            projects = projects,
            repos = repos,
            github = github,
            currentUserId = currentUserId,
            transient = transient,
            createdInviteToken = invite,
            instanceUrl = instance,
            workspaceDeleted = deleted,
            allWorkspaces = allWorkspaces,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspaceSettingsState())

    fun updateRole(memberId: String, role: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.updateRole(accountId, memberId, role) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the role") }
    }

    fun removeMember(memberId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.remove(accountId, memberId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't remove the member") }
    }

    fun createInvite(role: String = DomainContract.workspaceRoleMember) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { invitesApi.create(accountId, workspaceId, role) }
            .onSuccess { _createdInviteToken.value = it.token }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't create the invite") }
    }

    fun revokeInvite(id: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { invitesApi.revoke(accountId, id) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't revoke the invite") }
    }

    fun consumeCreatedInvite() {
        _createdInviteToken.value = null
    }

    fun deleteLabel(labelId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(accountId, workspaceId, labelId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the label") }
    }

    fun renameLabel(labelId: String, name: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(workspaceId, labelId, name = name)) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't rename the label") }
    }

    fun recolorLabel(labelId: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(workspaceId, labelId, color = color)) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't update the label") }
    }

    fun createLabel(name: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.create(accountId, CreateLabelInput(workspaceId, name.trim(), color)) }
            .onSuccess { created ->
                // Optimistic local upsert so the label appears immediately instead
                // of waiting for the labels shape's next poll (idempotent REPLACE;
                // Electric re-delivers the same row, so this is only a head-start).
                runCatching { holder.database(forAccountId = accountId).labelDao().upsert(created) }
            }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't create the label") }
    }

    fun consumeTransient() { _transient.value = null }

    fun deleteWorkspace() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { workspacesApi.delete(accountId, workspaceId) }
            .onSuccess { _workspaceDeleted.value = true }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the team") }
    }

    fun deleteProject(projectId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { workspacesApi.deleteProject(accountId, projectId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the project") }
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

    // Remove a repo from the registry. Blocked server-side (CONFLICT) while any
    // project still points at it — surface that message verbatim (masterplan §6).
    fun removeRepo(repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.remove(accountId, repositoryId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't remove the repository") }
        refreshRepos()
    }

    // Owner/admin: retarget a project's backing repo (projects.setRepository).
    fun setProjectRepository(projectId: String, repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, projectId, repositoryId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the repository") }
        refreshRepos()
    }
}
