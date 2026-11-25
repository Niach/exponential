package com.exponential.app.ui.settings

import android.content.Context
import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.PlaceholderStatus
import com.exponential.app.domain.GithubCopy
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.TeamImage
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.membersInJoinOrder
import com.exponential.app.domain.pendingInvites
import com.exponential.app.domain.placeholderStatuses
import com.exponential.app.data.api.BoardsApi
import com.exponential.app.data.api.DeletedIssue
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PutAwayBoard
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.GithubStatusResult
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.TeamMembersApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.UpdateTeamInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.GithubConnectStarter
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
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.repo.toRepoPr
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext

data class MemberRow(
    val member: TeamMemberEntity,
    val user: UserEntity?,
    // EXP-630: set while an email invite bound to this member is still
    // unaccepted — the row is on the roster but the person hasn't claimed it.
    val placeholder: PlaceholderStatus? = null,
)

data class TeamSettingsState(
    val team: TeamEntity? = null,
    /** In join order (EXP-1267 ×4, `membersInJoinOrder`). */
    val members: List<MemberRow> = emptyList(),
    /** The team's open invite links (unaccepted AND unexpired, `pendingInvites`),
     *  newest first — the owner-only "Pending invites" rows under the roster. */
    val pendingInvites: List<TeamInviteEntity> = emptyList(),
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
    // The connection block's ONE source, `integrations.github.status`
    // (mobile-marked URLs) — exactly as web and desktop: the VIEWER's own
    // GitHub connection and the installations their token sees. Null while
    // loading or after a failed probe.
    val githubStatus: GithubStatusResult? = null,
    // Review #14 (iOS `githubStatusFailed`): the probe failed with nothing
    // loaded — the block shows the quiet failed line with Retry.
    val githubStatusFailed: Boolean = false,
    // SLOP-26: a connect hop being minted (the ticket round-trip before the
    // Custom Tab opens) — the Connect/Reconnect pill is held meanwhile.
    val githubConnecting: Boolean = false,
    // EXP-1274: an already-loaded registry re-probing (Retry, resume, a
    // reconnect) — the section band spins its 12dp loading glyph.
    val reposRefreshing: Boolean = false,
    val currentUserId: String? = null,
    // FEED-79: the screen's failed action (a mutation, a load, a GitHub hop),
    // raised once as an error toast.
    val transient: FailedAction? = null,
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

/** #NEW-37: the Boards page's Archived and Trash bands (owners). */
data class PutAwayBoards(
    val archived: List<PutAwayBoard> = emptyList(),
    val trashed: List<PutAwayBoard> = emptyList(),
    // #EXP-1797: the team's issue trash (every member), newest deleted first.
    val deletedIssues: List<DeletedIssue> = emptyList(),
    val pendingId: String? = null,
)

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
    private val boardsApi: BoardsApi,
    private val issuesApi: IssuesApi,
    private val integrationsApi: IntegrationsApi,
    private val deepLinkBus: DeepLinkBus,
    private val connectStarter: GithubConnectStarter,
    @ApplicationContext private val appContext: Context,
    savedStateHandle: SavedStateHandle,
) : ViewModel() {

    /**
     * #EXP-1381 (settings-nav.json): the ONE category this page shows
     * (`team-settings/{section}`); it observes only that category's data. The
     * roster always loads (it decides who owns the team); null = every
     * category (no caller left, kept for previews).
     */
    val section: String? = savedStateHandle.get<String>("section")
    private fun observes(vararg sections: String): Boolean = section == null || section in sections

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
    // EXP-630: the team's UNACCEPTED invites (the DAO query filters accepted
    // ones out already) — the only thing that says a roster row is a
    // placeholder member waiting to be claimed.
    private val invitesFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null || !observes(SettingsNav.MEMBERS)) flowOf(emptyList())
        else db.teamInviteDao().observeByTeam(id)
    }
    private val labelsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null || !observes(SettingsNav.LABELS)) flowOf(emptyList())
        else db.labelDao().observeByTeam(id)
    }
    private val boardsFlow = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null || !observes(SettingsNav.BOARDS, SettingsNav.REPOSITORIES)) flowOf(emptyList())
        else db.boardDao().observeByTeam(id)
    }

    /** #EXP-1616 (repo-page.json): each repo row's checks rollup over its
     *  live PRs (`repoChecksRollup`), by repository id; absent = no chip. */
    val repoChecks: StateFlow<Map<String, String>> = dbAndSelected.flatMapLatest { (db, id) ->
        if (db == null || id == null || !observes(SettingsNav.REPOSITORIES)) flowOf(emptyList())
        else db.pullRequestDao().observeByTeam(id)
    }.map { prs ->
        prs.groupBy { it.repositoryId }.mapNotNull { (repo, rows) ->
            com.exponential.app.domain.RepoPage.repoChecksRollup(rows.map { it.toRepoPr() })?.let { repo to it }
        }.toMap()
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    private val _transient = MutableStateFlow<FailedAction?>(null)
    private val _teamDeleted = MutableStateFlow(false)
    private val _repos = MutableStateFlow<List<TeamRepo>>(emptyList())
    private val _reposLoaded = MutableStateFlow(false)
    private val _resolvingRepoId = MutableStateFlow<String?>(null)
    // FEED-32: ids a one-shot re-list already ran for — a repo the server
    // genuinely doesn't list (archived) must not loop the fetch.
    private val resolvedRepoIds = mutableSetOf<String>()
    private val _githubStatus = MutableStateFlow<GithubStatusResult?>(null)
    private val _githubStatusFailed = MutableStateFlow(false)
    private val _githubConnecting = MutableStateFlow(false)
    private val _reposRefreshing = MutableStateFlow(false)

    /** FEED-79: `Couldn't <verb> <thing>` + the server's message, Retry when [retry] is safe. */
    private fun fail(verb: String, thing: String, error: Throwable?, retry: (() -> Unit)? = null) {
        _transient.value = FailedAction(verb, thing, error?.let { trpcErrorMessage(it, "") }, retry = retry)
    }

    init {
        // Repositories aren't an Electric shape — (re)load the registry over
        // tRPC whenever the active account or selected team changes. The
        // The viewer's GitHub connection state rides along (the connection block).
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { a, w -> a to w }
                .collectLatest { (accountId, teamId) ->
                    _repos.value = emptyList()
                    _reposLoaded.value = false
                    _resolvingRepoId.value = null
                    resolvedRepoIds.clear()
                    _githubStatus.value = null
                    _githubStatusFailed.value = false
                    // Only Boards (the repo chips + retarget) and Repositories read the registry.
                    if (accountId != null && teamId != null && observes(SettingsNav.BOARDS, SettingsNav.REPOSITORIES)) {
                        // FEED-79: the list loads on its own (team switch,
                        // resume, after a write), so a failed load is quiet:
                        // the empty list, never a toast.
                        runCatching { repositoriesApi.list(accountId, teamId) }
                            .onSuccess { _repos.value = it }
                        _reposLoaded.value = true
                        loadGithubStatus(accountId, teamId)
                    }
                }
        }
        // The guided page / the App's setup page fire exponential://
        // github-connected — re-fetch so the block reflects the install
        // without leaving the screen. Event counter, not a consumed one-shot
        // (EXP-365): the repo picker may be collecting too, and both must
        // refresh. drop(1) skips the StateFlow replay. An error slug means the
        // hop FAILED: toast it instead of refreshing.
        viewModelScope.launch {
            deepLinkBus.githubConnected.drop(1).collect { event ->
                if (event.error != null) {
                    _transient.value = FailedAction("connect", "GitHub", GithubCopy.LINK_FAILED)
                } else {
                    refreshGithub()
                }
            }
        }
        // SLOP-26: the link-ticket hop's outcome (EXP-1126 link mode) — a
        // linked GitHub re-probes; a failure is a toast.
        viewModelScope.launch {
            auth.linkResult.drop(1).collect { result ->
                if (result.providerId != GithubConnectStarter.PROVIDER) return@collect
                if (result.error != null) {
                    _transient.value = FailedAction("connect", "GitHub", result.error)
                } else {
                    refreshGithub()
                }
            }
        }
    }

    /**
     * SLOP-26: Connect (or reconnect) the viewer's GitHub account — the
     * link-ticket hop, the guided page (`connectUrl`) as the fallback — handing
     * the URL to [open] (a Custom Tab). The outcome arrives on the deep links
     * collected above.
     */
    fun connectGithub(open: (String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return
        if (_githubConnecting.value) return
        _githubConnecting.value = true
        viewModelScope.launch {
            try {
                val hop = connectStarter.start(accountId, _githubStatus.value?.connectUrl)
                if (hop == null) {
                    _transient.value = FailedAction("connect", "GitHub", GithubCopy.LINK_FAILED) { connectGithub(open) }
                } else {
                    open(hop.url)
                }
            } catch (e: Exception) {
                _transient.value = FailedAction("connect", "GitHub", trpcErrorMessage(e, GithubCopy.LINK_FAILED)) {
                    connectGithub(open)
                }
            } finally {
                _githubConnecting.value = false
            }
        }
    }

    // Re-fetch the registry + grant state (bypassing the server's repo cache)
    // after a GitHub reconnect lands, or on screen resume as the deep-link
    // fallback (EXP-365 — a swallowed deep link left the row stale forever).
    // Failures keep the last good value (the toast shows only when there is
    // none) — also the toast's Retry. [retried] = the person's own Retry tap
    // on the failed line: its failure toasts (D10), the automatic re-reads
    // stay quiet.
    fun refreshGithub(retried: Boolean = false) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val teamId = selection.selectedId.value ?: return@launch
            _reposRefreshing.value = _reposLoaded.value
            try {
                runCatching { repositoriesApi.list(accountId, teamId) }
                    .onSuccess { _repos.value = it }
                loadGithubStatus(accountId, teamId, retried)
            } finally {
                _reposRefreshing.value = false
            }
        }
    }

    private suspend fun loadGithubStatus(accountId: String, teamId: String, retried: Boolean = false) {
        // FEED-79: re-read on open and on every resume, so a failure is quiet,
        // never a toast: the last good value stays, and with none the block
        // reads the failed line with Retry (iOS `githubStatusFailed`).
        runCatching { integrationsApi.githubStatus(accountId, teamId) }
            .onSuccess {
                _githubStatus.value = it
                _githubStatusFailed.value = false
            }
            .onFailure {
                if (it is CancellationException) throw it
                _githubStatusFailed.value = true
                if (retried) fail("load", "GitHub", it) { refreshGithub(retried = true) }
            }
    }

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
            // Quiet on failure (FEED-79): the row reads "Repository unavailable".
            runCatching { repositoriesApi.list(accountId, teamId) }
                .onSuccess { _repos.value = it }
            if (_resolvingRepoId.value == repositoryId) _resolvingRepoId.value = null
        }
    }

    // SLOP-26: disconnect the viewer's OWN GitHub account (confirm-first).
    // Repositories already added keep working; the connection state is
    // re-fetched either way so the block reflects the outcome.
    fun disconnectGithub() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { integrationsApi.githubDisconnect(accountId) }
            .onFailure { fail("disconnect", "GitHub", it) }
        refreshGithub()
    }

    val state: StateFlow<TeamSettingsState> = combine(
        listOf(
            teamFlow,
            membersFlow,
            invitesFlow,
            labelsFlow,
            boardsFlow,
            _repos,
            _githubStatus,
            if (observes(SettingsNav.MEMBERS)) dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() } else flowOf(emptyList()),
            auth.userId,
            auth.instanceUrl,
            _transient,
            _teamDeleted,
            auth.activeAccountId,
            _reposLoaded,
            _resolvingRepoId,
            _githubConnecting,
            _reposRefreshing,
            _githubStatusFailed,
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
        val githubStatus = values[6] as GithubStatusResult?
        @Suppress("UNCHECKED_CAST")
        val users = values[7] as List<UserEntity>
        val currentUserId = values[8] as String?
        val instance = values[9] as String?
        val transient = values[10] as FailedAction?
        val deleted = values[11] as Boolean
        val accountId = values[12] as String?
        val reposLoaded = values[13] as Boolean
        val resolvingRepoId = values[14] as String?
        val githubConnecting = values[15] as Boolean
        val reposRefreshing = values[16] as Boolean
        val githubStatusFailed = values[17] as Boolean
        val placeholders = placeholderStatuses(invites)
        TeamSettingsState(
            team = team,
            // Rows whose user hasn't synced yet (user == null) still render
            // (userDisplayName degrades to a "Member <id>" placeholder).
            members = membersInJoinOrder(members)
                .map { m ->
                    MemberRow(
                        m,
                        users.firstOrNull { it.id == m.userId },
                        placeholders[m.userId],
                    )
                },
            // Web `useTeamInvites`: newest invite first, then id (EXP-668).
            pendingInvites = pendingInvites(invites)
                .sortedWith(
                    compareByDescending<TeamInviteEntity> { WireTimestamps.parseEpochMs(it.createdAt) ?: Long.MIN_VALUE }
                        .thenBy { it.id },
                ),
            labels = labels,
            // Web `compareBoards` (EXP-525, the canonical order ×4):
            // sortOrder, then createdAt (a missing one first), then id.
            // Sorted here: `created_at` arrives as Postgres text OR ISO.
            boards = boards.sortedWith(
                compareBy<BoardEntity>({ it.sortOrder }, { WireTimestamps.parseEpochMs(it.createdAt) ?: Long.MIN_VALUE }, { it.id }),
            ),
            repos = repos,
            reposLoaded = reposLoaded,
            resolvingRepoId = resolvingRepoId,
            githubStatus = githubStatus,
            githubStatusFailed = githubStatusFailed,
            githubConnecting = githubConnecting,
            reposRefreshing = reposRefreshing,
            currentUserId = currentUserId,
            transient = transient,
            instanceUrl = instance,
            teamDeleted = deleted,
            accountId = accountId,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamSettingsState())

    fun updateRole(memberId: String, role: String): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.updateRole(accountId, memberId, role) }
            .onFailure { fail("change", "the role", it) { updateRole(memberId, role) } }
    }

    /** Owner-only (the rows are): kill a pending invite link. */
    fun revokeInvite(inviteId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { invitesApi.revoke(accountId, inviteId) }
            .onFailure { fail("revoke", "the invite", it) }
    }

    fun removeMember(memberId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.remove(accountId, memberId) }
            .onFailure { fail("remove", "the member", it) }
    }

    fun deleteLabel(labelId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(accountId, teamId, labelId) }
            .onFailure { fail("delete", "the label", it) }
    }

    fun renameLabel(labelId: String, name: String): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(teamId, labelId, name = name)) }
            .onFailure { fail("rename", "the label", it) { renameLabel(labelId, name) } }
    }

    fun recolorLabel(labelId: String, color: String): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(teamId, labelId, color = color)) }
            .onFailure { fail("update", "the label", it) { recolorLabel(labelId, color) } }
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
            .onFailure { fail("create", "the label", it) }
    }

    fun consumeTransient() { _transient.value = null }

    fun deleteTeam() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { teamsApi.delete(accountId, teamId) }
            .onSuccess { _teamDeleted.value = true }
            .onFailure { fail("delete", "the team", it) }
    }

    fun deleteBoard(boardId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { teamsApi.deleteBoard(accountId, boardId) }
            .onFailure { fail("delete", "the board", it) }
        refreshPutAway()
        // A trashed board's issues leave the issue trash with it.
        refreshDeletedIssues()
    }

    // --- #NEW-37: the archived boards and the 48h trash (owner-only tRPC
    // reads: both states are out of every synced shape, EXP-500). Loaded when
    // the Boards page opens and after every Unarchive / Restore / delete; a
    // failed read is quiet (FEED-79), the bands keep what they had. ---

    private val _putAway = MutableStateFlow(PutAwayBoards())
    val putAway: StateFlow<PutAwayBoards> = _putAway

    fun refreshPutAway(): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { boardsApi.listArchived(accountId, teamId) }
            .onSuccess { rows -> _putAway.value = _putAway.value.copy(archived = rows) }
        runCatching { boardsApi.listDeleted(accountId, teamId) }
            .onSuccess { rows -> _putAway.value = _putAway.value.copy(trashed = rows) }
    }

    fun unarchiveBoard(board: PutAwayBoard) = runPutAway(board, "unarchive") { accountId ->
        boardsApi.unarchive(accountId, board.id)
    }

    fun restoreBoard(board: PutAwayBoard) = runPutAway(board, "restore") { accountId ->
        boardsApi.restore(accountId, board.id)
    }

    // Refresh on success AND failure: an unarchive can fail because the board
    // was trashed meanwhile, a restore because it was purged.
    private fun runPutAway(board: PutAwayBoard, verb: String, action: suspend (String) -> Unit) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        _putAway.value = _putAway.value.copy(pendingId = board.id)
        runCatching { action(accountId) }.onFailure { fail(verb, board.name, it) }
        _putAway.value = _putAway.value.copy(pendingId = null)
        refreshPutAway().join()
        refreshDeletedIssues().join()
    }

    // --- #EXP-1797 (issue-lifecycle.json `trash`): the issue trash, a
    // member read (`issues.listDeleted`). Loaded when the Boards page opens
    // and after every Restore / Delete forever; a failed load is quiet (the
    // band keeps its rows), a failed action is the toast, and the list
    // reloads either way. ---

    fun refreshDeletedIssues(): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        runCatching { issuesApi.listDeleted(accountId, teamId) }
            .onSuccess { rows ->
                _putAway.value = _putAway.value.copy(deletedIssues = rows.sortedByDescending { it.deletedAt.orEmpty() })
            }
    }

    fun restoreIssue(issue: DeletedIssue) = runIssueTrash(issue, "restore") { accountId ->
        issuesApi.restore(accountId, issue.id)
    }

    /** Delete forever (owners), after the `purge-issue` prompt. */
    fun purgeIssue(issue: DeletedIssue) = runIssueTrash(issue, "delete") { accountId ->
        issuesApi.purge(accountId, issue.id)
    }

    private fun runIssueTrash(issue: DeletedIssue, verb: String, action: suspend (String) -> Unit) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        _putAway.value = _putAway.value.copy(pendingId = issue.id)
        runCatching { action(accountId) }.onFailure { fail(verb, issue.identifier, it) }
        _putAway.value = _putAway.value.copy(pendingId = null)
        refreshDeletedIssues().join()
    }

    // --- Repositories registry (server-only; the list is re-fetched after
    // every mutation because there is no Electric shape to sync it back). ---

    fun refreshRepos(): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = selection.selectedId.value ?: return@launch
        // Quiet on failure (FEED-79): every caller is automatic (a deep link,
        // after a write); the list keeps what it had.
        runCatching { repositoriesApi.list(accountId, teamId) }
            .onSuccess { _repos.value = it }
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
        refreshGithub()
    }

    /** #EXP-1616 (owners): create an Exponential repo (`repositories.create`);
     *  true = created (the sheet closes), a failure toasts and keeps it open. */
    suspend fun createRepository(name: String, defaultBranch: String, mirrorUrl: String?): Boolean {
        val accountId = auth.activeAccountId.value ?: return false
        val teamId = selection.selectedId.value ?: return false
        return try {
            repositoriesApi.create(accountId, teamId, name, defaultBranch, mirrorUrl)
            refreshRepos()
            true
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            fail("create", "the repository", t)
            false
        }
    }

    // Remove a repo from the registry (sharer-or-owner, EXP-557). Blocked
    // server-side (CONFLICT) while any board still points at it — the toast
    // carries that message verbatim (masterplan §6).
    fun removeRepo(repositoryId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.remove(accountId, repositoryId) }
            .onFailure { fail("remove", "the repository", it) }
        refreshRepos()
    }

    // Member-level (EXP-557): retarget a board's backing repo
    // (boards.setRepository). `null` detaches it. EXP-712: the server RESETS
    // the board's branch pin on every retarget (it belonged to the old repo),
    // which is exactly what the sheet shows afterwards.
    fun setBoardRepository(boardId: String, repositoryId: String?): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, boardId, repositoryId) }
            .onFailure { fail("change", "the board's repository", it) { setBoardRepository(boardId, repositoryId) } }
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
        }.onFailure { fail("connect", fullName, it) }
        if (added.isFailure) return@launch
        val repos = runCatching { repositoriesApi.list(accountId, teamId) }
            .onSuccess { _repos.value = it }
            .getOrNull() ?: return@launch
        val repo = repos.firstOrNull { it.fullName == fullName } ?: return@launch
        runCatching { repositoriesApi.setRepository(accountId, boardId, repo.id) }
            .onFailure { fail("change", "the board's repository", it) { setBoardRepository(boardId, repo.id) } }
        refreshRepos()
    }

    // EXP-712: the board's OWN branch (boards.update) — `null` clears the pin
    // so the board follows its repo's default branch again. The updated row
    // arrives over Electric.
    fun setBoardBranch(boardId: String, branch: String?): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { boardsApi.setDefaultBranch(accountId, boardId, branch) }
            .onFailure { fail("set", "the branch", it) { setBoardBranch(boardId, branch) } }
    }

    // --- General (web `TeamGeneralSection`): name, the merge mode, the team prompt.

    /** EXP-818: the name row saves itself on blur / Done; blank keeps the old name. */
    fun renameTeam(name: String): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val team = state.value.team ?: return@launch
        val next = name.trim()
        if (next.isEmpty() || next == team.name) return@launch
        _teamNameSaving.value = true
        runCatching { teamsApi.update(accountId, UpdateTeamInput(teamId = team.id, name = next)) }
            .onFailure { fail("save", "the team name", it) { renameTeam(name) } }
        _teamNameSaving.value = false
    }

    private val _teamNameSaving = MutableStateFlow(false)
    val teamNameSaving: StateFlow<Boolean> = _teamNameSaving

    private val _mergeModeBusy = MutableStateFlow(false)
    val mergeModeBusy: StateFlow<Boolean> = _mergeModeBusy

    /** #EXP-1710 (merge-train.json MODE): the team's contract teamMergeMode
     *  (`auto` | `approval`), a synced team field (owner-only). */
    fun setMergeMode(mode: String): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = state.value.team?.id ?: return@launch
        _mergeModeBusy.value = true
        runCatching { teamsApi.update(accountId, UpdateTeamInput(teamId = teamId, mergeMode = mode)) }
            .onFailure { fail("update", "the merge mode", it) { setMergeMode(mode) } }
        _mergeModeBusy.value = false
    }

    private val _zoneBusy = MutableStateFlow(false)
    val zoneBusy: StateFlow<Boolean> = _zoneBusy

    /** VAPP-201 (team-zone.json): the team time zone (null = each device's
     *  own), saved at once (owner-only); a failure = the failed-action toast. */
    fun setTimeZone(timeZone: String?): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val team = state.value.team ?: return@launch
        if (timeZone == team.timeZone) return@launch
        _zoneBusy.value = true
        runCatching { teamsApi.updateTimeZone(accountId, team.id, timeZone) }
            .onFailure { fail("save", "the time zone", it) { setTimeZone(timeZone) } }
        _zoneBusy.value = false
    }

    private val _imageBusy = MutableStateFlow(false)
    val imageBusy: StateFlow<Boolean> = _imageBusy

    /** #EXP-1645 (team-image.json): the picked image, one part `file` to the
     *  owner-only icon route; the shape delivers the new `icon_url`. */
    fun uploadTeamImage(uri: Uri): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = state.value.team?.id ?: return@launch
        _imageBusy.value = true
        runCatching {
            val bytes = withContext(Dispatchers.IO) {
                val known = MarkdownMediaUtils.querySize(appContext, uri)
                require(known == null || known <= TeamImage.Upload.MAX_BYTES) { "The image is too large" }
                requireNotNull(MarkdownMediaUtils.readBytes(appContext, uri)) { "The image could not be read" }
            }
            teamsApi.uploadIcon(
                accountId,
                teamId,
                bytes,
                MarkdownMediaUtils.guessFilename(appContext, uri),
                canonicalContentType(MarkdownMediaUtils.guessMimeType(appContext, uri)),
            )
        }.onFailure { fail("upload", "the team image", it) { uploadTeamImage(uri) } }
        _imageBusy.value = false
    }

    /** #EXP-1645: Remove, no confirm: `teams.update {iconUrl: null}`. */
    fun removeTeamImage(): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = state.value.team?.id ?: return@launch
        _imageBusy.value = true
        runCatching { teamsApi.removeIcon(accountId, teamId) }
            .onFailure { fail("remove", "the team image", it) { removeTeamImage() } }
        _imageBusy.value = false
    }

    /** EXP-1025: the team prompt editor (server-only text, fetched on open). */
    data class TeamPromptState(
        /** Null = loading (or failed: [loadFailed]). */
        val text: String? = null,
        val saved: String = "",
        val loadFailed: Boolean = false,
        val saving: Boolean = false,
        val justSaved: Boolean = false,
    )

    private val _prompt = MutableStateFlow(TeamPromptState())
    val prompt: StateFlow<TeamPromptState> = _prompt
    private var promptTeamId: String? = null
    private var promptSaveJob: Job? = null
    private var promptFailedText: String? = null

    init {
        if (observes(SettingsNav.GENERAL)) {
            viewModelScope.launch {
                combine(auth.activeAccountId, selection.selectedId) { a, t -> a to t }
                    .collectLatest { (accountId, teamId) -> loadPrompt(accountId, teamId, retried = false) }
            }
        }
    }

    /** FEED-79: the load runs on open, so a failure is quiet (the field's
     *  Retry); only a failed Retry tap toasts. */
    fun retryPromptLoad() {
        viewModelScope.launch { loadPrompt(auth.activeAccountId.value, selection.selectedId.value, retried = true) }
    }

    private suspend fun loadPrompt(accountId: String?, teamId: String?, retried: Boolean) {
        promptSaveJob?.cancel()
        _prompt.value = TeamPromptState()
        promptTeamId = null
        if (accountId == null || teamId == null) return
        runCatching { teamsApi.getAgentPrompt(accountId, teamId) }
            .onSuccess {
                promptTeamId = teamId
                _prompt.value = TeamPromptState(text = it.agentPrompt, saved = it.agentPrompt)
            }
            .onFailure {
                if (it is CancellationException) throw it
                _prompt.value = TeamPromptState(loadFailed = true)
                if (retried) fail("load", "the team prompt", it) { retryPromptLoad() }
            }
    }

    /** D9: autosaves [TEAM_PROMPT_DEBOUNCE_MS] after the last keystroke. */
    fun editPrompt(text: String) {
        val current = _prompt.value
        if (current.text == null) return
        _prompt.value = current.copy(text = text)
        promptSaveJob?.cancel()
        promptSaveJob = viewModelScope.launch {
            kotlinx.coroutines.delay(TEAM_PROMPT_DEBOUNCE_MS)
            savePrompt()
        }
    }

    /** Blur / leaving the page: save a dirty, under-cap draft now. */
    fun flushPrompt() {
        promptSaveJob?.cancel()
        viewModelScope.launch { savePrompt() }
    }

    private suspend fun savePrompt() {
        val accountId = auth.activeAccountId.value ?: return
        val teamId = promptTeamId ?: return
        val current = _prompt.value
        val text = current.text ?: return
        if (text.trimEnd() == current.saved) return
        if (text.toByteArray(Charsets.UTF_8).size > TEAM_PROMPT_MAX_BYTES) return
        if (text == promptFailedText) return
        _prompt.value = current.copy(saving = true, justSaved = false)
        runCatching { teamsApi.update(accountId, UpdateTeamInput(teamId = teamId, agentPrompt = text)) }
            .onSuccess {
                promptFailedText = null
                _prompt.value = _prompt.value.copy(saved = text.trimEnd(), saving = false, justSaved = true)
                viewModelScope.launch {
                    kotlinx.coroutines.delay(TEAM_PROMPT_SAVED_HOLD_MS)
                    _prompt.value = _prompt.value.copy(justSaved = false)
                }
            }
            .onFailure {
                if (it is CancellationException) throw it
                promptFailedText = text
                _prompt.value = _prompt.value.copy(saving = false)
                fail("save", "the team prompt", it) {
                    promptFailedText = null
                    flushPrompt()
                }
            }
    }

    override fun onCleared() {
        // Leaving inside the debounce window must not drop the edit.
        val accountId = auth.activeAccountId.value
        val teamId = promptTeamId
        val current = _prompt.value
        val text = current.text
        if (accountId != null && teamId != null && text != null && text.trimEnd() != current.saved &&
            text.toByteArray(Charsets.UTF_8).size <= TEAM_PROMPT_MAX_BYTES && text != promptFailedText
        ) {
            teamPromptFlushScope.launch {
                runCatching { teamsApi.update(accountId, UpdateTeamInput(teamId = teamId, agentPrompt = text)) }
            }
        }
        super.onCleared()
    }

    // --- Members: invite by email / add without inviting (web `InviteMemberForm`).

    data class InviteFormState(
        val sending: Boolean = false,
        val adding: Boolean = false,
        /** The address the server refused, under the field. */
        val emailError: String? = null,
        /** A mail that could not go out: the link to share by hand. */
        val inviteUrl: String? = null,
        /** EXP-725: the team is at its seat cap — the form is gone. */
        val capped: Boolean = false,
        /** Bumped on a success: the form clears its fields. */
        val sent: Int = 0,
    )

    private val _inviteForm = MutableStateFlow(InviteFormState())
    val inviteForm: StateFlow<InviteFormState> = _inviteForm

    fun clearInviteError() {
        _inviteForm.value = _inviteForm.value.copy(emailError = null)
    }

    fun sendInvite(email: String, name: String, onToast: (String) -> Unit): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = state.value.team?.id ?: return@launch
        val to = email.trim()
        if (to.isEmpty()) return@launch
        _inviteForm.value = _inviteForm.value.copy(sending = true, emailError = null)
        runCatching { invitesApi.createForEmail(accountId, teamId, to, name.trim().ifEmpty { null }) }
            .onSuccess { result ->
                if (result.emailDelivered == true) {
                    onToast("Invite sent to $to")
                    _inviteForm.value = _inviteForm.value.copy(sending = false, inviteUrl = null, sent = _inviteForm.value.sent + 1)
                } else {
                    _inviteForm.value = _inviteForm.value.copy(
                        sending = false,
                        inviteUrl = com.exponential.app.domain.WebLinks.invite(auth.instanceUrl.value, result.token),
                    )
                    fail("email", "the invite", null)
                }
            }
            .onFailure { inviteFailure(it, "send", "the invite") }
    }

    fun addWithoutInviting(email: String, name: String, onToast: (String) -> Unit): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val teamId = state.value.team?.id ?: return@launch
        val to = email.trim()
        if (to.isEmpty()) return@launch
        _inviteForm.value = _inviteForm.value.copy(adding = true, emailError = null)
        runCatching { membersApi.addPlaceholder(accountId, teamId, to, name.trim().ifEmpty { null }) }
            .onSuccess { added ->
                onToast("Added $added to the team")
                _inviteForm.value = _inviteForm.value.copy(adding = false, inviteUrl = null, sent = _inviteForm.value.sent + 1)
            }
            .onFailure { inviteFailure(it, "add", "the member") }
    }

    private fun inviteFailure(error: Throwable, verb: String, thing: String) {
        if (error is CancellationException) throw error
        val form = _inviteForm.value.copy(sending = false, adding = false)
        _inviteForm.value = when {
            com.exponential.app.data.api.isPlanLimitError(error) -> form.copy(capped = true)
            (error as? com.exponential.app.data.api.TrpcException)?.code == "BAD_REQUEST" ->
                form.copy(emailError = trpcErrorMessage(error, INVALID_EMAIL))
            else -> {
                fail(verb, thing, error)
                form
            }
        }
    }

    // --- Repositories: the branch chip (EXP-462).

    fun setRepoDefaultBranch(repositoryId: String, branch: String?): Job = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { repositoriesApi.setDefaultBranch(accountId, repositoryId, branch) }
            .onFailure { fail("set", "the default branch", it) { setRepoDefaultBranch(repositoryId, branch) } }
        refreshRepos()
    }
}

// Outlives the ViewModel (the voiceSettingsFlushScope idiom): the leave-page flush.
private val teamPromptFlushScope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + Dispatchers.IO)

/** EXP-1025: the team prompt's cap in UTF-8 bytes (contract `team.agentPromptMaxBytes`). */
const val TEAM_PROMPT_MAX_BYTES = 12288
/** feedback-states.json `autosave`. */
private const val TEAM_PROMPT_DEBOUNCE_MS = 800L
private const val TEAM_PROMPT_SAVED_HOLD_MS = 2000L
/** feedback-states.json's invalid-email input case. */
private const val INVALID_EMAIL = "Enter a full email address"
