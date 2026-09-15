package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.githubConnectErrorMessage
import com.exponential.app.data.api.BoardsApi
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.GithubStatusResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.TeamMembersApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.TeamEntity
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
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class MemberRow(val member: TeamMemberEntity, val user: UserEntity?)

data class TeamSettingsState(
    val team: TeamEntity? = null,
    val members: List<MemberRow> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val boards: List<BoardEntity> = emptyList(),
    // Server-only repositories registry, loaded over tRPC (never synced).
    val repos: List<TeamRepo> = emptyList(),
    // FEED-32: false until the first `repositories.list` attempt for the
    // selected team returned — a linked board reads "Loading repository…"
    // rather than "unavailable" while that first list is still in flight.
    val reposLoaded: Boolean = false,
    // FEED-32: the repo id a one-shot re-list is resolving right now (a board
    // whose synced repositoryId the registry copy doesn't know yet).
    val resolvingRepoId: String? = null,
    // FEED-42: the connection block's ONE source, `integrations.github.status`
    // (mobile-marked URLs) — exactly as web and desktop: the viewer's accounts
    // plus, for owners, the team's STALE links (the `repos` payload never
    // carries `stale`). Null while loading or after a failed probe.
    val githubStatus: GithubStatusResult? = null,
    // The status probe failed (and no earlier value is on screen) — the block
    // says so with a Retry instead of rendering nothing.
    val githubFailed: Boolean = false,
    // FEED-42: registry/GitHub failures (list load, remove, unlink, a failed
    // connect hop) render INLINE above the list, never as a snackbar.
    val repositoriesError: String? = null,
    val currentUserId: String? = null,
    val transient: String? = null,
    val instanceUrl: String? = null,
    val teamDeleted: Boolean = false,
    // Active account id — the repo picker sheet (GithubRepoPickerSheet) takes
    // it as a parameter (EXP-225).
    val accountId: String? = null,
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
    private val labelsApi: LabelsApi,
    private val teamsApi: TeamsApi,
    private val repositoriesApi: RepositoriesApi,
    private val boardsApi: BoardsApi,
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
    private val labelsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.labelDao().observeByTeam(id)
    }
    private val boardsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null) flowOf(emptyList()) else db.boardDao().observeByTeam(id)
    }

    private val _transient = MutableStateFlow<String?>(null)
    private val _teamDeleted = MutableStateFlow(false)
    private val _repos = MutableStateFlow<List<TeamRepo>>(emptyList())
    private val _reposLoaded = MutableStateFlow(false)
    private val _resolvingRepoId = MutableStateFlow<String?>(null)
    // FEED-32: ids a one-shot re-list already ran for — a repo the server
    // genuinely doesn't list (archived) must not loop the fetch.
    private val resolvedRepoIds = mutableSetOf<String>()
    private val _githubStatus = MutableStateFlow<GithubStatusResult?>(null)
    private val _githubFailed = MutableStateFlow(false)
    private val _repositoriesError = MutableStateFlow<String?>(null)
    val transient: StateFlow<String?> = _transient.asStateFlow()

    init {
        // Repositories aren't an Electric shape — (re)load the registry over
        // tRPC whenever the active account or selected team changes. The
        // GitHub grant state rides along (needsReauth drives the reconnect row).
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { a, w -> a to w }
                .collectLatest { (accountId, teamId) ->
                    _repos.value = emptyList()
                    _reposLoaded.value = false
                    _resolvingRepoId.value = null
                    resolvedRepoIds.clear()
                    _githubStatus.value = null
                    _githubFailed.value = false
                    _repositoriesError.value = null
                    if (accountId != null && teamId != null) {
                        // Failures surface inline instead of silently
                        // rendering "No repositories connected yet." / hiding
                        // the whole GitHub block (EXP-365, FEED-42).
                        runCatching { repositoriesApi.list(accountId, teamId) }
                            .onSuccess { _repos.value = it }
                            .onFailure { _repositoriesError.value = trpcErrorMessage(it, "Couldn’t load repositories") }
                        _reposLoaded.value = true
                        loadGithubStatus(accountId, teamId)
                    }
                }
        }
        // The reconnect Custom Tab ends on the server's "connected" page, which
        // fires exponential://github-connected — re-fetch so the needsReauth row clears
        // without leaving the screen. Event counter, not a consumed one-shot
        // (EXP-365): the repo picker may be collecting too, and both must
        // refresh. drop(1) skips the StateFlow replay. An error slug means the
        // connect FAILED (EXP-390): surface it instead of refreshing.
        viewModelScope.launch {
            deepLinkBus.githubConnected.drop(1).collect { event ->
                if (event.error != null) {
                    _repositoriesError.value = githubConnectErrorMessage(event.error)
                } else {
                    refreshGithub()
                }
            }
        }
    }

    // Re-fetch the registry + grant state (bypassing the server's repo cache)
    // after a GitHub reconnect lands, or on screen resume as the deep-link
    // fallback (EXP-365 — a swallowed deep link left the row stale forever).
    // Failures keep the last good value (the failed line shows only when
    // there is none) — also the failed state's Retry.
    fun refreshGithub() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val teamId = selection.selectedId.value ?: return@launch
            runCatching { repositoriesApi.list(accountId, teamId) }
                .onSuccess { _repos.value = it }
            loadGithubStatus(accountId, teamId)
        }
    }

    private suspend fun loadGithubStatus(accountId: String, teamId: String) {
        runCatching { integrationsApi.githubStatus(accountId, teamId) }
            .onSuccess {
                _githubStatus.value = it
                _githubFailed.value = false
            }
            .onFailure { if (_githubStatus.value == null) _githubFailed.value = true }
    }

    fun clearRepositoriesError() { _repositoriesError.value = null }

    // FEED-32: a board's synced repositoryId can point at a repo this registry
    // copy has never seen — the settings sheet connects a new repo and the
    // LIVE board row flips before the list refreshed, or another client
    // connected it. Re-list ONCE per unknown id (never loop) while the row
    // reads "Loading repository…"; still unknown afterwards reads
    // "Repository unavailable".
    fun ensureRepoKnown(repositoryId: String) {
        if (_repos.value.any { it.id == repositoryId }) return
        if (!resolvedRepoIds.add(repositoryId)) return
        val accountId = auth.activeAccountId.value ?: return
        val teamId = selection.selectedId.value ?: return
        _resolvingRepoId.value = repositoryId
        viewModelScope.launch {
            runCatching { repositoriesApi.list(accountId, teamId) }
                .onSuccess { _repos.value = it }
                .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't load repositories") }
            if (_resolvingRepoId.value == repositoryId) _resolvingRepoId.value = null
        }
    }

    // Disconnect a linked GitHub account from the team (EXP-557 — the visible
    // "Disconnect account" action on STALE accounts, which no reconnect can
    // heal). Link-creator-or-owner server-side; the grant state is re-fetched
    // either way so the section reflects the outcome.
    fun unlinkGithub(installationId: Long) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        _repositoriesError.value = null
        runCatching { integrationsApi.githubUnlink(accountId, teamId, installationId) }
            .onFailure { _repositoriesError.value = trpcErrorMessage(it, "Couldn’t disconnect the GitHub account") }
        refreshGithub()
    }

    val state: StateFlow<TeamSettingsState> = combine(
        listOf(
            teamFlow,
            membersFlow,
            labelsFlow,
            boardsFlow,
            _repos,
            _githubStatus,
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            auth.userId,
            auth.instanceUrl,
            _transient,
            _teamDeleted,
            auth.activeAccountId,
            _reposLoaded,
            _resolvingRepoId,
            _githubFailed,
            _repositoriesError,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val team = values[0] as TeamEntity?
        @Suppress("UNCHECKED_CAST")
        val members = values[1] as List<TeamMemberEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[2] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val boards = values[3] as List<BoardEntity>
        @Suppress("UNCHECKED_CAST")
        val repos = values[4] as List<TeamRepo>
        val githubStatus = values[5] as GithubStatusResult?
        @Suppress("UNCHECKED_CAST")
        val users = values[6] as List<UserEntity>
        val currentUserId = values[7] as String?
        val instance = values[8] as String?
        val transient = values[9] as String?
        val deleted = values[10] as Boolean
        val accountId = values[11] as String?
        val reposLoaded = values[12] as Boolean
        val resolvingRepoId = values[13] as String?
        val githubFailed = values[14] as Boolean
        val repositoriesError = values[15] as String?
        TeamSettingsState(
            team = team,
            // Rows whose user hasn't synced yet (user == null) still render
            // (userDisplayName degrades to a "Member <id>" placeholder).
            members = members
                .map { m -> MemberRow(m, users.firstOrNull { it.id == m.userId }) },
            labels = labels,
            boards = boards,
            repos = repos,
            reposLoaded = reposLoaded,
            resolvingRepoId = resolvingRepoId,
            githubStatus = githubStatus,
            githubFailed = githubFailed,
            repositoriesError = repositoriesError,
            currentUserId = currentUserId,
            transient = transient,
            instanceUrl = instance,
            teamDeleted = deleted,
            accountId = accountId,
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
            .onFailure { _repositoriesError.value = trpcErrorMessage(it, "Couldn’t load repositories") }
    }

    // Member-level since EXP-557: register a repo picked from the caller's own
    // GitHub connection in the registry (repositories.add — web parity,
    // EXP-225; connecting shares it with the team), then re-fetch. FEED-42:
    // SUSPENDS and THROWS — the Add-repository sheet awaits it and keeps
    // itself open with the failure inline, dismissing only on success.
    suspend fun addRepository(fullName: String, defaultBranch: String, isPrivate: Boolean) {
        val accountId = auth.activeAccountId.value ?: return
        val teamId = selection.selectedId.value ?: return
        repositoriesApi.add(accountId, teamId, fullName, defaultBranch, isPrivate)
        _repositoriesError.value = null
        refreshGithub()
    }

    // Remove a repo from the registry (sharer-or-owner, EXP-557). Blocked
    // server-side (CONFLICT) while any board still points at it — surface that
    // message verbatim (masterplan §6).
    fun removeRepo(repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        _repositoriesError.value = null
        runCatching { repositoriesApi.remove(accountId, repositoryId) }
            .onFailure { _repositoriesError.value = trpcErrorMessage(it, "Couldn’t remove the repository") }
        refreshRepos()
    }

    // Member-level (EXP-557): retarget a board's backing repo
    // (boards.setRepository). `null` detaches it. EXP-712: the server RESETS
    // the board's branch pin on every retarget (it belonged to the old repo),
    // which is exactly what the sheet shows afterwards.
    fun setBoardRepository(boardId: String, repositoryId: String?) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, boardId, repositoryId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the repository") }
        refreshRepos()
    }

    // EXP-712: connect a brand-new repo (the board settings' "Connect another
    // repository…") and point the board at it in one gesture — the registry
    // add has no board argument, so the fresh row is resolved by full name off
    // the refreshed list.
    fun connectBoardRepository(
        boardId: String,
        fullName: String,
        defaultBranch: String,
        isPrivate: Boolean,
    ) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        val added = runCatching {
            repositoriesApi.add(accountId, teamId, fullName, defaultBranch, isPrivate)
        }.onFailure { _transient.value = trpcErrorMessage(it, "Couldn't add the repository") }
        if (added.isFailure) return@launch
        val repos = runCatching { repositoriesApi.list(accountId, teamId) }
            .onSuccess { _repos.value = it }
            .getOrNull() ?: return@launch
        val repo = repos.firstOrNull { it.fullName == fullName } ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, boardId, repo.id) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the repository") }
        refreshRepos()
    }

    // EXP-712: the board's OWN branch (boards.update) — `null` clears the pin
    // so the board follows its repo's default branch again. The updated row
    // arrives over Electric.
    fun setBoardBranch(boardId: String, branch: String?) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { boardsApi.setDefaultBranch(accountId, boardId, branch) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't change the branch") }
    }
}
