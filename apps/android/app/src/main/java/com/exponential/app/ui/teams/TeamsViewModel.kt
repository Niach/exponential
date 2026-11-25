package com.exponential.app.ui.teams

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AppCreateInput
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.BoardsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.MultiAccountTeamRepository
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.ownedTeamIdsFlow
import com.exponential.app.data.sidebar.SidebarFoldersStore
import com.exponential.app.data.sidebar.isTeamOpen
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.domain.SidebarMenus
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SidebarTree
import com.exponential.app.domain.WebLinks
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import java.util.UUID
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

private val FOLDS = setOf(SidebarTree.ROW_TEAM, SidebarTree.ROW_FOLDER)

/** What a leaf row of the tree opens. */
sealed interface TeamTarget {
    val name: String

    data class Board(val board: BoardEntity) : TeamTarget {
        override val name: String get() = board.name
    }
    data class Agent(val agent: AgentDefinitionEntity) : TeamTarget {
        override val name: String get() = agent.name
    }
    data class App(val app: AppEntity) : TeamTarget {
        override val name: String get() = app.name
    }
    data class Session(val session: SessionEntity, override val name: String) : TeamTarget
}

/** One row of the Teams tab; [target] null = a team or a folder. */
data class TeamTreeRow(
    val row: SidebarTree.PhoneTreeRow,
    val team: TeamEntity,
    val target: TeamTarget? = null,
) {
    val name: String
        get() = when (row.kind) {
            SidebarTree.ROW_TEAM -> team.name
            SidebarTree.ROW_FOLDER -> row.name.orEmpty()
            else -> target?.name.orEmpty()
        }

    /** A fold: a team or a folder. */
    val group: Boolean get() = target == null
}

/** Another signed-in server's teams ([server] = its name, under each). */
data class TeamsGroup(val accountId: String, val server: String, val teams: List<TeamEntity>)

data class TeamsState(
    /** False until the first read landed. */
    val loaded: Boolean = false,
    /** The active account's tree. */
    val rows: List<TeamTreeRow> = emptyList(),
    val others: List<TeamsGroup> = emptyList(),
    /** The teams the viewer owns. */
    val owned: Set<String> = emptySet(),
    /** `phone.empty`: the team whose empty state shows under the group. */
    val emptyTeamId: String? = null,
    /** Per team what a layout write runs over. */
    val trees: Map<String, SidebarTree.PhoneTreeTeam> = emptyMap(),
    /** Every leaf by id, folded ones too (a board's header). */
    val leaves: Map<String, TeamTreeRow> = emptyMap(),
    /** The leaf ids a team or a folder hides while folded, by its id. */
    val inside: Map<String, List<String>> = emptyMap(),
    /** The folder whose name is in edit. */
    val editingId: String? = null,
) {
    /** nav-badges.json `teamBadge` / `sumBadges`: a team's or a folder's badge;
     *  [needsYou] = the teams' shares, [badgeOf] = a leaf's own badge. */
    fun groupBadge(
        row: SidebarTree.PhoneTreeRow,
        needsYou: Map<String, Int>,
        badgeOf: (leafId: String) -> NavBadges.Badge,
    ): NavBadges.Badge = when {
        row.kind == SidebarTree.ROW_TEAM -> teamBadge(row.id, row.open, needsYou, badgeOf)
        row.open -> NavBadges.NONE
        else -> NavBadges.sumBadges(inside[row.id].orEmpty().map(badgeOf))
    }

    private fun teamBadge(teamId: String, open: Boolean, needsYou: Map<String, Int>, badgeOf: (leafId: String) -> NavBadges.Badge) =
        NavBadges.teamBadge(needsYou[teamId] ?: 0, open, inside[teamId].orEmpty().map(badgeOf))

    /** The Teams tab's badge: its team rows, each folded. */
    fun tabBadge(needsYou: Map<String, Int>, badgeOf: (leafId: String) -> NavBadges.Badge): NavBadges.Badge =
        NavBadges.sumBadges(trees.keys.map { teamBadge(it, open = false, needsYou, badgeOf) })

    /** A folder Move to folder offers, the team's rows from its top level. */
    fun moveTargets(row: TeamTreeRow): SidebarMenus.MoveTargets {
        val tree = trees[row.team.id] ?: return SidebarMenus.MoveTargets(emptyList(), null)
        val rows = SidebarTree.folderRows(tree.copy(closedFolders = emptySet()))
        return SidebarMenus.moveTargets(rows.map { SidebarMenus.MoveRow(it.kind, it.id, it.depth, it.name) }, row.row.id)
    }
}

/**
 * #EXP-2109 (sidebar-tree.json `phone`): THE Teams tab: the member teams of the
 * active account as ONE nested tree (the member's own order from the synced
 * `sidebar_layout` section `team`), then the other signed-in servers' teams.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class TeamsViewModel @Inject constructor(
    private val auth: AuthRepository,
    multiAccountTeams: MultiAccountTeamRepository,
    holder: DatabaseHolder,
    private val store: SidebarFoldersStore,
    private val appsApi: AppsApi,
    private val boardsApi: BoardsApi,
) : ViewModel() {
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private fun <T> scoped(empty: T, query: (ExponentialDatabase) -> Flow<T>): Flow<T> = dbFlow.scopedQuery(empty, query)

    private data class Groups(val mine: List<TeamEntity>, val others: List<TeamsGroup>)

    private val groups: Flow<Groups> = combine(multiAccountTeams.serverGroups, auth.activeAccountId) { list, active ->
        fun ordered(teams: List<TeamEntity>): List<TeamEntity> {
            val byId = teams.associateBy { it.id }
            return SidebarTree.teams(teams.map { SidebarTree.Team(it.id, it.name) }).mapNotNull(byId::get)
        }
        Groups(
            mine = ordered(list.firstOrNull { it.accountId == active }?.teams.orEmpty()),
            others = list.filter { it.accountId != active }.map { TeamsGroup(it.accountId, it.hostname, ordered(it.teams)) }
                .filter { it.teams.isNotEmpty() },
        )
    }

    /** The placed targets' session rows (an id that is no session finds none). */
    private val placedSessions: Flow<List<SessionEntity>> = store.teamEntries
        .map { teams -> teams.values.flatten().mapNotNull { it.targetId }.distinct().sorted() }
        .distinctUntilChanged()
        .flatMapLatest { ids ->
            if (ids.isEmpty()) flowOf(emptyList()) else scoped(emptyList()) { it.sessionDao().observeByIds(ids) }
        }

    private val titles: Flow<Map<String, String>> = placedSessions.flatMapLatest { rows ->
        if (rows.isEmpty()) return@flatMapLatest flowOf(emptyMap())
        val ids = rows.mapTo(HashSet()) { it.id }
        scoped(emptyList()) { it.sessionIssueDao().observeAll() }
            .map { links -> links.filter { it.sessionId in ids } }
            .distinctUntilChanged()
            .flatMapLatest { links ->
                scoped(emptyList()) { it.issueDao().observeByIds(links.map { link -> link.issueId }.distinct()) }.map { issues ->
                    val titler = SessionTree.titler(
                        links.map(SessionTree::sessionIssue),
                        issues.map { SessionTree.issue(it) },
                        rows.flatMap { Chats.digestMessages(it.id, Chats.parseDigest(it.digest)) },
                    )
                    rows.associate { it.id to titler(SessionTree.session(it)) }
                }
            }
    }

    private data class Targets(
        val boards: List<BoardEntity>,
        val agents: List<AgentDefinitionEntity>,
        val apps: List<AppEntity>,
        val sessions: List<SessionEntity>,
        val titles: Map<String, String>,
    )

    private val targets: Flow<Targets> = combine(
        scoped(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
        scoped(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() },
        scoped(emptyList<AppEntity>()) { it.appDao().observeAll() },
        placedSessions,
        titles,
    ) { boards, agents, apps, sessions, names ->
        // chats.json `sidebarAgents`: an app's own agent never shows.
        val owned = apps.mapNotNullTo(HashSet()) { it.agentId }
        Targets(boards.filter { it.deletedAt == null }, agents.filter { it.id !in owned }, apps, sessions, names)
    }

    private val editing = MutableStateFlow<String?>(null)

    private val local = combine(store.openFlags, editing, ownedTeamIdsFlow(dbFlow, auth.userId), ::Triple)

    val state: StateFlow<TeamsState> = combine(groups, store.teamEntries, targets, local) { g, layouts, t, (flags, editingId, owned) ->
        val boards = t.boards.groupBy { it.teamId }
        val agents = t.agents.groupBy { it.teamId }
        val apps = t.apps.groupBy { it.teamId }
        val trees = g.mine.map { team ->
            val entries = layouts[team.id].orEmpty()
            val items = SidebarTree.teamItems(
                teamId = team.id,
                boards = boards[team.id].orEmpty().map { SidebarTree.Sortable(it.id, it.sortOrder) },
                agents = agents[team.id].orEmpty().map { SidebarTree.Sortable(it.id, it.sortOrder, it.role) },
                apps = apps[team.id].orEmpty().map { SidebarTree.Named(it.id, it.name) },
                sessions = t.sessions.map { SidebarTree.SessionLike(it.id, it.teamId, it.parentId) },
                entries = entries,
            )
            SidebarTree.PhoneTreeTeam(
                id = team.id,
                items = items,
                entries = entries,
                open = flags.isTeamOpen(team.id),
                closedFolders = entries.filter { it.kind == SidebarFolders.KIND_FOLDER && flags[SidebarFolders.openKey(team.id, it.id)] == false }
                    .mapTo(HashSet()) { it.id },
            )
        }
        val teams = g.mine.associateBy { it.id }
        val boardById = t.boards.associateBy { it.id }
        val agentById = t.agents.associateBy { it.id }
        val appById = t.apps.associateBy { it.id }
        val sessionById = t.sessions.associateBy { it.id }
        val pure = SidebarTree.phoneTreeRows(trees)
        fun resolve(row: SidebarTree.PhoneTreeRow) = TeamTreeRow(
            row = row,
            team = teams.getValue(row.teamId),
            target = when (row.kind) {
                    SidebarTree.Kind.Board.wire -> boardById[row.id]?.let { TeamTarget.Board(it) }
                    SidebarTree.Kind.Agent.wire -> agentById[row.id]?.let { TeamTarget.Agent(it) }
                    SidebarTree.Kind.App.wire -> appById[row.id]?.let { TeamTarget.App(it) }
                    SidebarTree.Kind.Session.wire -> sessionById[row.id]?.let { TeamTarget.Session(it, t.titles[it.id].orEmpty()) }
                    else -> null
                },
        )
        val rows = withEditingFolder(pure, trees, editingId).map(::resolve).filter { it.target != null || it.row.kind in FOLDS }
        val unfolded = SidebarTree.phoneTreeRows(trees.map { it.copy(open = true, closedFolders = emptySet()) })
        val leaves = unfolded.filter { it.kind !in FOLDS }.associate { it.id to resolve(it) }
        TeamsState(
            loaded = true,
            rows = rows,
            others = g.others,
            owned = owned,
            emptyTeamId = SidebarTree.phoneTeamsEmpty(pure.filter { it.kind == SidebarTree.ROW_TEAM }.map { it.id to it.count }),
            trees = trees.associateBy { it.id },
            leaves = leaves,
            inside = leavesInside(unfolded),
            editingId = editingId,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamsState())

    /** A new folder is empty: SHOWN at its team's end while it is named. */
    private fun withEditingFolder(
        rows: List<SidebarTree.PhoneTreeRow>,
        trees: List<SidebarTree.PhoneTreeTeam>,
        editingId: String?,
    ): List<SidebarTree.PhoneTreeRow> {
        if (editingId == null || rows.any { it.id == editingId }) return rows
        val team = trees.firstOrNull { tree -> tree.open && tree.entries.any { it.id == editingId } } ?: return rows
        val folder = team.entries.first { it.id == editingId }
        val at = rows.indexOfLast { it.teamId == team.id } + 1
        val row = SidebarTree.PhoneTreeRow(
            SidebarTree.ROW_FOLDER, folder.id, team.id, 1, folder.name ?: SidebarFolders.DEFAULT_FOLDER_NAME, open = true, count = 0,
        )
        return rows.take(at) + row + rows.drop(at)
    }

    /** Each fold's leaves off the fully unfolded [rows]. */
    private fun leavesInside(rows: List<SidebarTree.PhoneTreeRow>): Map<String, List<String>> {
        val out = HashMap<String, MutableList<String>>()
        val open = ArrayDeque<SidebarTree.PhoneTreeRow>()
        for (row in rows) {
            while (open.isNotEmpty() && open.last().depth >= row.depth) open.removeLast()
            if (row.kind in FOLDS) open.addLast(row) else open.forEach { out.getOrPut(it.id) { mutableListOf() } += row.id }
        }
        return out
    }

    /** A team of another server: that account first, every screen re-scopes. */
    fun useAccount(accountId: String) {
        if (accountId != auth.activeAccountId.value) auth.switchAccount(accountId)
    }

    fun openTeam(teamId: String) = store.openTeam(teamId)

    fun toggle(row: TeamTreeRow) {
        if (row.row.kind == SidebarTree.ROW_TEAM) store.toggleTeam(row.team.id) else store.toggle(row.team.id, row.row.id)
    }

    // ── The layout's writes ─────────────────────────────────────────────────

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    private fun attempt(verb: String, thing: String, retry: () -> Unit, block: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                block()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""), retry = retry)
            }
        }
    }

    private fun save(teamId: String, writes: SidebarFolders.Writes?) {
        if (writes == null) return
        editing.value = writes.createdFolderId ?: editing.value
        attempt("save", "the sidebar", { save(teamId, writes) }) { store.save(teamId, writes) }
    }

    private fun tree(teamId: String) = state.value.trees[teamId]

    /** New folder: an empty one at the team's end, its name in edit. */
    fun addFolder(teamId: String) {
        val tree = tree(teamId) ?: return
        store.openTeam(teamId)
        val id = UUID.randomUUID().toString()
        val top = SidebarFolders.tree(tree.items.map { it.id }, tree.entries).size
        save(
            teamId,
            SidebarFolders.Writes(
                upserts = listOf(
                    SidebarFolders.EntryWrite(id, SidebarFolders.KIND_FOLDER, null, null, SidebarFolders.DEFAULT_FOLDER_NAME, (top + 1).toDouble()),
                ),
                deletes = emptyList(),
                createdFolderId = id,
            ),
        )
    }

    fun moveInto(teamId: String, id: String, folderId: String) = move(teamId, SidebarFolders.Move(id, into = folderId))

    /** Top level: right after the folder holding it. */
    fun moveOut(teamId: String, id: String, folderId: String) = move(teamId, SidebarFolders.Move(id, after = folderId))

    private fun move(teamId: String, move: SidebarFolders.Move) {
        val tree = tree(teamId) ?: return
        save(teamId, SidebarFolders.move(tree.items.map { it.id }, tree.entries, move, UUID.randomUUID().toString()))
    }

    fun moveToNewFolder(teamId: String, id: String) {
        val tree = tree(teamId) ?: return
        save(teamId, SidebarFolders.moveToNewFolder(tree.items.map { it.id }, tree.entries, id, UUID.randomUUID().toString()))
    }

    fun ungroup(teamId: String, folderId: String) {
        val tree = tree(teamId) ?: return
        save(teamId, SidebarFolders.ungroup(tree.items.map { it.id }, tree.entries, folderId))
    }

    fun edit(folderId: String?) {
        editing.value = folderId
    }

    /** The in-place name is done. */
    fun rename(teamId: String, folderId: String, name: String) {
        if (editing.value == folderId) editing.value = null
        val current = tree(teamId)?.entries?.firstOrNull { it.id == folderId } ?: return
        if (current.name == SidebarFolders.folderName(name)) return
        attempt("rename", "the folder", { rename(teamId, folderId, name) }) { store.rename(teamId, folderId, name) }
    }

    fun removeFromSidebar(teamId: String, targetId: String) {
        attempt("save", "the sidebar", { removeFromSidebar(teamId, targetId) }) { store.remove(teamId, targetId) }
    }

    private val _archiving = MutableStateFlow(false)
    val archiving: StateFlow<Boolean> = _archiving

    fun archive(board: BoardEntity, onDone: (archived: Boolean) -> Unit) {
        if (_archiving.value) return
        val accountId = auth.activeAccountId.value ?: return
        _archiving.value = true
        viewModelScope.launch {
            var archived = false
            try {
                boardsApi.archive(accountId, board.id)
                archived = true
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("archive", "the board", trpcErrorMessage(t, "")) { archive(board) {} }
            } finally {
                _archiving.value = false
                onDone(archived)
            }
        }
    }

    private var creating = false

    /** New › App: a fresh draft, opened in its editor. */
    fun createApp(teamId: String, onCreated: (appId: String) -> Unit) {
        if (creating) return
        val accountId = auth.activeAccountId.value ?: return
        creating = true
        viewModelScope.launch {
            try {
                onCreated(appsApi.create(accountId, AppCreateInput(teamId = teamId)).id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("create", "the app", trpcErrorMessage(t, "")) { createApp(teamId, onCreated) }
            } finally {
                creating = false
            }
        }
    }

    /** The row's page on the web (Copy link); null = none. */
    fun link(row: TeamTreeRow): String? {
        val base = auth.instanceUrl.value ?: return null
        return when (val target = row.target) {
            is TeamTarget.Board -> WebLinks.boardUrl(base, row.team.slug, target.board.slug)
            is TeamTarget.App -> WebLinks.appUrl(base, row.team.slug, target.app.slug)
            is TeamTarget.Session -> WebLinks.sessionUrl(base, row.team.slug, target.session.id)
            else -> null
        }
    }
}
