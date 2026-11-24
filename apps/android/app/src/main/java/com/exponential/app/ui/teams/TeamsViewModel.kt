package com.exponential.app.ui.teams

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AppCreateInput
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.MultiAccountTeamRepository
import com.exponential.app.data.db.ServerTeamGroup
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.SidebarLayoutEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.TeamMemberEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.Chats
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SidebarTree
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** #EXP-2109: the Teams tab's root: the member teams, by server. */
@HiltViewModel
class TeamsViewModel @Inject constructor(
    private val auth: AuthRepository,
    multiAccountTeams: MultiAccountTeamRepository,
) : ViewModel() {
    /** Null until the first read landed. */
    val groups: StateFlow<List<ServerTeamGroup>?> = multiAccountTeams.serverGroups
        .map<List<ServerTeamGroup>, List<ServerTeamGroup>?> { groups ->
            groups.map { group ->
                val byId = group.teams.associateBy { it.id }
                group.copy(teams = SidebarTree.teams(group.teams.map { SidebarTree.Team(it.id, it.name) }).mapNotNull(byId::get))
            }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** A team of another server: that account first, every screen re-scopes. */
    fun useAccount(accountId: String) {
        if (accountId != auth.activeAccountId.value) auth.switchAccount(accountId)
    }
}

/** One row of a phone team page, resolved against its synced target. */
sealed interface TeamPageRow {
    val id: String

    data class Folder(override val id: String, val name: String) : TeamPageRow
    data class Board(val board: BoardEntity) : TeamPageRow {
        override val id: String get() = board.id
    }
    data class Agent(val agent: AgentDefinitionEntity) : TeamPageRow {
        override val id: String get() = agent.id
    }
    data class App(val app: AppEntity) : TeamPageRow {
        override val id: String get() = app.id
    }
    data class Session(val session: SessionEntity, val title: String) : TeamPageRow {
        override val id: String get() = session.id
    }
}

data class TeamPageState(
    val team: TeamEntity? = null,
    /** The folder page's name. */
    val folderName: String? = null,
    /** Null = the folder is gone (the page pops). */
    val rows: List<TeamPageRow>? = emptyList(),
    val owner: Boolean = false,
    val loaded: Boolean = false,
    /** The team is gone (left or deleted): the page pops. */
    val gone: Boolean = false,
)

/**
 * #EXP-2109 (sidebar-tree.json `phone`): one page of the Teams tab: a team's
 * mixed list (`folderId` absent) or one of its folders, the member's own order
 * from the synced `sidebar_layout` section `team`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class TeamPageViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val appsApi: AppsApi,
) : ViewModel() {
    val teamId: String = savedStateHandle["teamId"] ?: ""
    private val folderId: String? = savedStateHandle["folderId"]

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private fun <T> scoped(empty: T, query: (ExponentialDatabase) -> Flow<T>): Flow<T> = dbFlow.scopedQuery(empty, query)

    private val entries = scoped(emptyList<SidebarLayoutEntity>()) { it.sidebarLayoutDao().observe(teamId, SidebarTree.SECTION) }
        .map { rows -> rows.map(SidebarLayoutEntity::toEntry) }

    /** The placed targets' session rows (an id that is no session finds none). */
    private val placedSessions: Flow<List<SessionEntity>> = entries
        .map { rows -> rows.mapNotNull { it.targetId }.distinct() }
        .distinctUntilChanged()
        .flatMapLatest { ids ->
            if (ids.isEmpty()) flowOf(emptyList()) else scoped(emptyList()) { it.sessionDao().observeByIds(ids) }
        }

    private data class Targets(
        val boards: List<BoardEntity>,
        val agents: List<AgentDefinitionEntity>,
        val apps: List<AppEntity>,
        val sessions: List<SessionEntity>,
        val titles: Map<String, String>,
    )

    private val titles: Flow<Map<String, String>> = combine(
        placedSessions,
        scoped(emptyList()) { it.sessionIssueDao().observeAll() },
        scoped(emptyList()) { it.issueDao().observeByTeam(teamId) },
    ) { rows, links, issues ->
        if (rows.isEmpty()) return@combine emptyMap()
        val ids = rows.mapTo(HashSet()) { it.id }
        val titler = SessionTree.titler(
            links.filter { it.sessionId in ids }.map(SessionTree::sessionIssue),
            issues.map { SessionTree.issue(it) },
            rows.flatMap { Chats.digestMessages(it.id, Chats.parseDigest(it.digest)) },
        )
        rows.associate { it.id to titler(SessionTree.session(it)) }
    }

    private val targets: Flow<Targets> = combine(
        scoped(emptyList<BoardEntity>()) { it.boardDao().observeByTeam(teamId) },
        scoped(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeByTeam(teamId) },
        scoped(emptyList<AppEntity>()) { it.appDao().observeByTeam(teamId) },
        placedSessions,
        titles,
    ) { boards, agents, apps, sessions, names ->
        // chats.json `sidebarAgents`: an app's own agent never shows.
        val owned = apps.mapNotNullTo(HashSet()) { it.agentId }
        Targets(boards.filter { it.deletedAt == null }, agents.filter { it.id !in owned }, apps, sessions, names)
    }

    private val members = scoped(emptyList<TeamMemberEntity>()) { it.teamMemberDao().observeByTeam(teamId) }

    val state: StateFlow<TeamPageState> = combine(
        scoped(null as List<TeamEntity>?) { it.teamDao().observeAll().map<List<TeamEntity>, List<TeamEntity>?> { rows -> rows } },
        entries,
        targets,
        members,
        auth.userId,
    ) { teams, layout, t, memberRows, me ->
        val boards = t.boards.associateBy { it.id }
        val agents = t.agents.associateBy { it.id }
        val apps = t.apps.associateBy { it.id }
        val sessions = t.sessions.associateBy { it.id }
        val items = SidebarTree.teamItems(
            teamId = teamId,
            boards = t.boards.map { SidebarTree.Sortable(it.id, it.sortOrder) },
            agents = t.agents.map { SidebarTree.Sortable(it.id, it.sortOrder, it.role) },
            apps = t.apps.map { SidebarTree.Named(it.id, it.name) },
            sessions = t.sessions.map { SidebarTree.SessionLike(it.id, it.teamId, it.parentId) },
            entries = layout,
        )
        val rows = SidebarTree.phoneTeamPage(items, layout, folderId)?.mapNotNull { row ->
            when (row.kind) {
                null -> TeamPageRow.Folder(row.id, row.name.orEmpty())
                SidebarTree.Kind.Board -> boards[row.id]?.let { TeamPageRow.Board(it) }
                SidebarTree.Kind.Agent -> agents[row.id]?.let { TeamPageRow.Agent(it) }
                SidebarTree.Kind.App -> apps[row.id]?.let { TeamPageRow.App(it) }
                SidebarTree.Kind.Session -> sessions[row.id]?.let { TeamPageRow.Session(it, t.titles[it.id].orEmpty()) }
            }
        }
        val team = teams?.firstOrNull { it.id == teamId }
        TeamPageState(
            team = team,
            folderName = folderId?.let { id -> layout.firstOrNull { it.id == id }?.name },
            rows = rows,
            owner = me != null && memberRows.any { it.userId == me && it.role == DomainContract.teamRoleOwner },
            loaded = true,
            gone = !teams.isNullOrEmpty() && team == null,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamPageState())

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    private var creating = false

    /** New › App: a fresh draft, opened in its editor. */
    fun createApp(onCreated: (appId: String) -> Unit) {
        if (creating) return
        creating = true
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                onCreated(appsApi.create(accountId, AppCreateInput(teamId = teamId)).id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("create", "the app", trpcErrorMessage(t, "")) { createApp(onCreated) }
            } finally {
                creating = false
            }
        }
    }
}
