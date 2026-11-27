package com.exponential.app.ui.teams

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.RepositoryEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.domain.SidebarMenus
import com.exponential.app.domain.SidebarTree
import com.exponential.app.ui.components.PageHeader

// #EXP-2171 (sidebar-tree.json `repositoryRow`): the Teams tab with a team's
// repository rows (the checks dot, the GitHub mark) and a repository row's
// long-press menu, drawn by the real tree and menu sheet over fixed rows
// (device pictures without a backend).

const val TEAMS_REPOSITORY_SPECIMEN_TEST_ID = "teams-repository-specimen"

private val TEAM = TeamEntity(id = "t1", name = "Acme", slug = "acme", createdAt = "", updatedAt = "")

private fun board(id: String, name: String, order: Double) = BoardEntity(
    id = id, teamId = "t1", name = name, slug = id, prefix = id.uppercase(), color = "#6366f1", icon = "square-kanban",
    sortOrder = order, createdAt = "", updatedAt = "",
)

private fun repo(id: String, provider: String, fullName: String, order: Double) =
    RepositoryEntity(id = id, teamId = "t1", provider = provider, fullName = fullName, slug = fullName.substringAfter('/'), sortOrder = order)

/** The specimen's tree: two boards, four repositories (failing · passing ·
 *  a GitHub one without checks · a hosted one without), one in a folder. */
internal fun teamsRepositorySpecimenState(): TeamsState {
    val boards = listOf(board("b1", "Mobile App", 1.0), board("b2", "Website", 2.0))
    val repos = listOf(
        TeamTarget.Repository(repo("r1", "ecode", "acme/engine", 0.0), "failure"),
        TeamTarget.Repository(repo("r2", "github", "acme-inc/site", 0.0), "success"),
        TeamTarget.Repository(repo("r3", "github", "acme-inc/docs", 1.0), null),
        TeamTarget.Repository(repo("r4", "ecode", "acme/infra", 1.0), "pending"),
    )
    val items = SidebarTree.teamItems(
        teamId = TEAM.id,
        boards = boards.map { SidebarTree.Sortable(it.id, it.sortOrder) },
        repositories = repos.map { SidebarTree.Repo(it.repository.id, it.repository.sortOrder, it.repository.fullName) },
        agents = emptyList(),
        apps = emptyList(),
        sessions = emptyList(),
        entries = emptyList(),
    )
    val entries = listOf(
        SidebarFolders.Entry("f1", SidebarFolders.KIND_FOLDER, null, null, "Code", 3.0),
        SidebarFolders.Entry("e1", SidebarFolders.KIND_ITEM, "r4", "f1", null, 1.0),
    )
    val tree = SidebarTree.PhoneTreeTeam(TEAM.id, items, entries, open = true, closedFolders = emptySet())
    val byId = boards.associate { it.id to TeamTarget.Board(it) } + repos.associateBy { it.repository.id }
    val rows = SidebarTree.phoneTreeRows(listOf(tree)).map { row -> TeamTreeRow(row, TEAM, byId[row.id]) }
    return TeamsState(loaded = true, rows = rows, owned = setOf(TEAM.id), trees = mapOf(TEAM.id to tree))
}

/** The Teams tab's page over [teamsRepositorySpecimenState]; [menuOpen] =
 *  the `acme/engine` row's long-press menu up. */
@Composable
fun TeamsRepositorySpecimenScreen(menuOpen: Boolean = false) {
    val state = teamsRepositorySpecimenState()
    Box(Modifier.fillMaxSize().testTag(TEAMS_REPOSITORY_SPECIMEN_TEST_ID)) {
        Column(Modifier.statusBarsPadding()) {
            PageHeader(title = Chats.Copy.HEADERS.getValue("teams"))
            Box(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                TeamsTree(
                    state = state,
                    needsYou = emptyMap(),
                    badgeOf = { NavBadges.NONE },
                    marks = emptyMap(),
                    onOpen = {},
                    onMenu = {},
                    onRename = { _, _ -> },
                    newMenu = { emptyList() },
                    moreMenu = { emptyList() },
                    onTeamTop = { _, _ -> },
                )
            }
        }
        if (menuOpen) {
            val groups = SidebarMenus.phoneMenuGroups(SidebarTree.Kind.Repository.wire).map { group ->
                group.map { verb ->
                    menuItem(
                        verb,
                        SidebarMenus.COPY.getValue(verb),
                        SidebarMenus.ICONS.getValue(verb),
                        submenu = if (verb == "move") emptyList() else null,
                    )
                }
            }
            TeamRowMenuSheet(title = SidebarTree.repositoryName("acme/engine"), groups = groups, onDismiss = {})
        }
    }
}
