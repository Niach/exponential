package com.exponential.app.domain

/**
 * #EXP-2109 (fixtures/sidebar-tree.json, its `$comment` IS the spec; the pure
 * reference is `packages/domain-contract/src/sidebar-tree.ts`): the phone's
 * Teams tab, one page per level (Teams › team › folder), over the member's
 * synced `sidebar_layout` section [SECTION]. Read-only: nothing drags, nothing
 * is created. `SidebarTreeTest` replays `teams`, `teamItems`, `teamsMenu` and
 * `phone`.
 */
object SidebarTree {
    /** The one mixed list per team. */
    const val SECTION = "team"

    /** The Teams page's + menu. */
    val TEAMS_MENU: List<String> = listOf("new-team", "join-team")

    object Copy {
        const val TEAM_ACTIONS = "Team actions"
        const val NEW = "New"
        const val SETTINGS = "Settings"
        const val NEW_TEAM = "New team"
        const val JOIN_TEAM = "Join team"
        val KINDS: Map<Kind, String> = linkedMapOf(Kind.Board to "Board", Kind.Agent to "Agent", Kind.App to "App", Kind.Session to "Session")
    }

    enum class Kind(val wire: String) { Board("board"), Agent("agent"), App("app"), Session("session") }

    data class Team(val id: String, val name: String, val kind: String = "team")

    data class Sortable(val id: String, val sortOrder: Double, val role: String? = null)

    data class Named(val id: String, val name: String)

    data class SessionLike(val id: String, val teamId: String, val parentId: String?)

    data class Item(val id: String, val kind: Kind)

    private val byName = compareBy<Named>({ it.name.lowercase(java.util.Locale.ENGLISH) }, { it.id })

    /** The viewer's member teams by name; the personal team never shows. */
    fun teams(teams: List<Team>): List<String> =
        teams.filter { it.kind != "personal" }.map { Named(it.id, it.name) }.sortedWith(byName).map { it.id }

    /** A team's targets in DEFAULT order: boards, agents (never an Assistant),
     *  apps, then the team's root sessions the member placed. */
    fun teamItems(
        teamId: String,
        boards: List<Sortable>,
        agents: List<Sortable>,
        apps: List<Named>,
        sessions: List<SessionLike>,
        entries: List<SidebarFolders.Entry>,
    ): List<Item> {
        val bySort = compareBy<Sortable>({ it.sortOrder }, { it.id })
        val byId = sessions.associateBy { it.id }
        val placed = entries
            .filter { it.kind == SidebarFolders.KIND_ITEM && it.targetId != null }
            .sortedWith(compareBy<SidebarFolders.Entry>({ it.sortOrder }, { it.id }))
            .mapNotNull { byId[it.targetId] }
            .filter { it.teamId == teamId && it.parentId == null }
            .distinct()
        return boards.sortedWith(bySort).map { Item(it.id, Kind.Board) } +
            agents.filter { it.role == DomainContract.agentRoleAgent }.sortedWith(bySort).map { Item(it.id, Kind.Agent) } +
            apps.sortedWith(byName).map { Item(it.id, Kind.App) } +
            placed.map { Item(it.id, Kind.Session) }
    }

    /** A phone team page's `…` menu. */
    data class MenuItem(val id: String, val children: List<Kind>)

    fun phoneTeamMenu(owner: Boolean): List<MenuItem> = buildList {
        if (owner) add(MenuItem("new", listOf(Kind.Board, Kind.Agent, Kind.App)))
        add(MenuItem("settings", emptyList()))
    }

    /** One row of a phone team page: a folder ([kind] null, its [name]) or an item. */
    data class PhoneRow(val kind: Kind?, val id: String, val name: String?)

    /** One page of the Teams tab: a team's top level ([folderId] null) or a
     *  folder's children; null = the folder is gone. */
    fun phoneTeamPage(items: List<Item>, entries: List<SidebarFolders.Entry>, folderId: String?): List<PhoneRow>? {
        val kinds = items.associate { it.id to it.kind }
        fun item(id: String) = PhoneRow(kinds.getValue(id), id, null)
        val tree = SidebarFolders.tree(items.map { it.id }, entries)
        if (folderId == null) {
            return tree.map { node ->
                when (node) {
                    is SidebarFolders.Node.Folder -> PhoneRow(null, node.id, node.name)
                    is SidebarFolders.Node.Item -> item(node.id)
                }
            }
        }
        return (tree.firstOrNull { it is SidebarFolders.Node.Folder && it.id == folderId } as? SidebarFolders.Node.Folder)
            ?.children?.map(::item)
    }
}
