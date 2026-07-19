package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.TeamMembersApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.TeamInviteEntity
import com.exponential.app.data.db.TeamMemberEntity
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

data class MemberRow(val member: TeamMemberEntity, val user: UserEntity?)

data class TeamSettingsState(
    val team: TeamEntity? = null,
    val members: List<MemberRow> = emptyList(),
    val invites: List<TeamInviteEntity> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val boards: List<BoardEntity> = emptyList(),
    // Server-only repositories registry, loaded over tRPC (never synced).
    val repos: List<TeamRepo> = emptyList(),
    // GitHub grant state (integrations.github.repos, mobile-marked URLs) —
    // drives the "Reconnect GitHub" affordance when a linked installation has
    // no captured grants (needsReauth). Null while loading / not configured.
    val github: GithubReposResult? = null,
    val currentUserId: String? = null,
    val transient: String? = null,
    val createdInviteToken: String? = null,
    val instanceUrl: String? = null,
    val teamDeleted: Boolean = false,
) {
    // Owner-gated controls key off this (hidden for non-owners — web parity).
    val isOwner: Boolean
        get() = currentUserId != null && members.any {
            it.member.userId == currentUserId && it.member.role == DomainContract.teamRoleOwner
        }
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class TeamSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: TeamSelection,
    private val holder: DatabaseHolder,
    private val membersApi: TeamMembersApi,
    private val invitesApi: TeamInvitesApi,
    private val labelsApi: LabelsApi,
    private val teamsApi: TeamsApi,
    private val repositoriesApi: RepositoriesApi,
    private val integrationsApi: IntegrationsApi,
    private val deepLinkBus: DeepLinkBus,
) : ViewModel() {

    // Reactive account scoping: a Settings → Teams tap on a different
    // server switches the active account and this ViewModel re-scopes to the
    // new account's DB automatically (no rebuild, no pending-handoff flag).
    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val dbAndSelected = combine(dbFlow, selection.selectedId) { db, id -> db to id }

    private val teamFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(null)
        else db.teamDao().observeAll().map { list -> list.firstOrNull { it.id == id } }
    }

    private val membersFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(id)
    }
    private val invitesFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.teamInviteDao().observeByTeam(id)
    }
    private val labelsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.labelDao().observeByTeam(id)
    }
    private val boardsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.boardDao().observeByTeam(id)
    }

    private val _transient = MutableStateFlow<String?>(null)
    private val _createdInviteToken = MutableStateFlow<String?>(null)
    private val _teamDeleted = MutableStateFlow(false)
    private val _repos = MutableStateFlow<List<TeamRepo>>(emptyList())
    private val _github = MutableStateFlow<GithubReposResult?>(null)
    val transient: StateFlow<String?> = _transient.asStateFlow()

    init {
        // Repositories aren't an Electric shape — (re)load the registry over
        // tRPC whenever the active account or selected team changes. The
        // GitHub grant state rides along (needsReauth drives the reconnect row).
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { a, w -> a to w }
                .collectLatest { (accountId, teamId) ->
                    _repos.value = emptyList()
                    _github.value = null
                    if (accountId != null && teamId != null) {
                        runCatching { repositoriesApi.list(accountId, teamId) }
                            .onSuccess { _repos.value = it }
                        runCatching { integrationsApi.githubRepos(accountId, teamId) }
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
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { repositoriesApi.list(accountId, teamId) }
            .onSuccess { _repos.value = it }
        runCatching { integrationsApi.githubRepos(accountId, teamId, refresh = true) }
            .onSuccess { _github.value = it }
    }

    val state: StateFlow<TeamSettingsState> = combine(
        listOf(
            teamFlow,
            membersFlow,
            invitesFlow,
            labelsFlow,
            boardsFlow,
            _repos,
            _github,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            auth.userId,
            auth.instanceUrl,
            _transient,
            _createdInviteToken,
            _teamDeleted,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val team = values[0] as TeamEntity?
        @Suppress("UNCHECKED_CAST")
        val members = values[1] as List<TeamMemberEntity>
        @Suppress("UNCHECKED_CAST")
        val invites = values[2] as List<TeamInviteEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[3] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val boards = values[4] as List<BoardEntity>
        @Suppress("UNCHECKED_CAST")
        val repos = values[5] as List<TeamRepo>
        val github = values[6] as GithubReposResult?
        @Suppress("UNCHECKED_CAST")
        val users = values[7] as List<UserEntity>
        val currentUserId = values[8] as String?
        val instance = values[9] as String?
        val transient = values[10] as String?
        val invite = values[11] as String?
        val deleted = values[12] as Boolean
        TeamSettingsState(
            team = team,
            // Synthetic agent users (widget reporters etc.) are team
            // members server-side but never shown in the roster — iOS hides them
            // too. Rows whose user hasn't synced yet (user == null) still render
            // (userDisplayName degrades to a "Member <id>" placeholder).
            members = members
                .map { m -> MemberRow(m, users.firstOrNull { it.id == m.userId }) }
                .filter { it.user?.isAgent != true },
            invites = invites,
            labels = labels,
            boards = boards,
            repos = repos,
            github = github,
            currentUserId = currentUserId,
            transient = transient,
            createdInviteToken = invite,
            instanceUrl = instance,
            teamDeleted = deleted,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamSettingsState())

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

    fun createInvite(role: String = DomainContract.teamRoleMember, email: String? = null) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        val trimmedEmail = email?.trim()?.takeIf { it.isNotEmpty() }
        runCatching { invitesApi.create(accountId, teamId, role, email = trimmedEmail) }
            .onSuccess { result ->
                _createdInviteToken.value = result.token
                // Delivery feedback (EXP-188 invite-by-email): the invite row is
                // created either way; a failed/unconfigured send falls back to
                // sharing the link by hand.
                if (trimmedEmail != null) {
                    _transient.value = if (result.emailDelivered == true) {
                        "Invite sent to $trimmedEmail"
                    } else {
                        "Couldn't email $trimmedEmail — share the invite link instead"
                    }
                }
            }
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
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(accountId, teamId, labelId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the label") }
    }

    fun renameLabel(labelId: String, name: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(teamId, labelId, name = name)) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't rename the label") }
    }

    fun recolorLabel(labelId: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(teamId, labelId, color = color)) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't update the label") }
    }

    fun createLabel(name: String, color: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.create(accountId, CreateLabelInput(teamId, name.trim(), color)) }
            .onSuccess { created ->
                // Optimistic local upsert so the label appears immediately instead
                // of waiting for the labels shape's next poll (idempotent REPLACE;
                // Electric re-delivers the same row, so this is only a head-start).
                runCatching { holder.database(forAccountId = accountId).labelDao().upsert(created) }
            }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't create the label") }
    }

    fun consumeTransient() { _transient.value = null }

    fun deleteTeam() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { teamsApi.delete(accountId, teamId) }
            .onSuccess { _teamDeleted.value = true }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the team") }
    }

    fun deleteBoard(boardId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { teamsApi.deleteBoard(accountId, boardId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't delete the board") }
    }

    // --- Repositories registry (server-only; the list is re-fetched after
    // every mutation because there is no Electric shape to sync it back). ---

    fun refreshRepos() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { repositoriesApi.list(accountId, teamId) }
            .onSuccess { _repos.value = it }
            .onFailure { _transient.value = it.message }
    }

    // Remove a repo from the registry. Blocked server-side (CONFLICT) while any
    // board still points at it — surface that message verbatim (masterplan §6).
    fun removeRepo(repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.remove(accountId, repositoryId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't remove the repository") }
        refreshRepos()
    }

    // Owner/admin: retarget a board's backing repo (boards.setRepository).
    fun setBoardRepository(boardId: String, repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, boardId, repositoryId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the repository") }
        refreshRepos()
    }
}
