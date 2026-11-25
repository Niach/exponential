package com.exponential.app.domain

/**
 * #EXP-2109 (fixtures/sidebar-tree.json, its `$comment` IS the spec; the pure
 * reference is `packages/domain-contract/src/sidebar-tree.ts`): the phone's
 * Teams tab, ONE page holding the nested tree (team, folder, item), over the
 * member's synced `sidebar_layout` section [SECTION]. `SidebarTreeTest` replays
 * `teams`, `teamItems`, `teamsMenu` and `phone`.
 */
object SidebarTree {
    /** The one mixed list per team. */
    const val SECTION = "team"

    /** The Teams page's + menu. */
    val TEAMS_MENU: List<String> = listOf("new-team", "join-team")

    /** A team's device-local expanded flag; no stored flag = OPEN. */
    fun teamOpenKey(teamId: String): String = "sidebar-team-open:$teamId"

    object Copy {
        const val TEAM_ACTIONS = "Team actions"
        const val SETTINGS = "Settings"
        const val NEW_TEAM = "New team"
        const val JOIN_TEAM = "Join team"
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

    /** One row of the Teams tab; [kind] = `team`, `folder` or a [Kind]'s wire. */
    data class PhoneTreeRow(
        val kind: String,
        val id: String,
        val teamId: String,
        val depth: Int,
        /** A folder's name; null on the rest. */
        val name: String?,
        val open: Boolean,
        val count: Int,
    )

    const val ROW_TEAM = "team"
    const val ROW_FOLDER = "folder"

    data class PhoneTreeTeam(
        val id: String,
        val items: List<Item>,
        val entries: List<SidebarFolders.Entry>,
        val open: Boolean,
        val closedFolders: Set<String>,
    )

    /** A team's `sidebarRows` (depth 0 = its top level). */
    fun folderRows(team: PhoneTreeTeam): List<SidebarFolders.Row> =
        SidebarFolders.rows(SidebarFolders.tree(team.items.map { it.id }, team.entries)) { it !in team.closedFolders }

    /** THE Teams tab: the teams in [teams]' order, each its row and (open) its
     *  nodes one level deeper, with the item's kind. */
    fun phoneTreeRows(teams: List<PhoneTreeTeam>): List<PhoneTreeRow> = teams.flatMap { team ->
        val kinds = team.items.associate { it.id to it.kind.wire }
        val rows = folderRows(team)
        val head = PhoneTreeRow(ROW_TEAM, team.id, team.id, 0, null, team.open, rows.count { it.depth == 0 })
        if (!team.open) {
            listOf(head)
        } else {
            listOf(head) + rows.map { row ->
                PhoneTreeRow(
                    kind = if (row.kind == SidebarFolders.KIND_FOLDER) ROW_FOLDER else kinds.getValue(row.id),
                    id = row.id,
                    teamId = team.id,
                    depth = row.depth + 1,
                    name = row.name,
                    open = row.open,
                    count = row.count,
                )
            }
        }
    }

    /** The viewer's ONLY team without a node (id, count): its empty state
     *  shows under the group; null = none. */
    fun phoneTeamsEmpty(teams: List<Pair<String, Int>>): String? =
        teams.singleOrNull()?.takeIf { it.second == 0 }?.first
}
