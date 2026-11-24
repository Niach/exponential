package com.exponential.app.ui.teams

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SidebarTree
import com.exponential.app.ui.apps.AppGlyph
import com.exponential.app.ui.chat.AgentGlyph
import com.exponential.app.ui.chat.ChatMenuPanel
import com.exponential.app.ui.chats.ChatsViewModel
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.CountBadge
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderCircle
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SidebarFolderGlyph
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.onboarding.TeamSetupSheet
import com.exponential.app.ui.settings.SettingsRow
import com.exponential.app.ui.theme.glassGroup

// #EXP-2109 (sidebar-tree.json `phone`): THE TEAMS TAB, one page per level.
// TEAMS = the member teams in `teams`' order (mark, name, needs-you count,
// chevron; the header's + opens `teamsMenu`) › TEAM = its mixed list in the
// member's own order › FOLDER = a folder's children. Read-only; rows are the
// phone settings look (a glass group of rows).

/** A row's glyph: the settings rows' lead slot. */
private val RowGlyph = 22.dp

object TeamsTestIds {
    const val TEAMS = "teams-page"
    const val TEAM_ROW = "team-row"
    const val TEAM = "team-page"
    const val ITEM_ROW = "team-item-row"
    const val FOLDER_ROW = "team-folder-row"
    const val TEAM_SETTINGS = "team-settings"
    const val ADD = "teams-add"
    const val MORE = "team-more"
}

/** One server's teams as the root page lists them: the active account's, then
 *  the other signed-in servers' ([server] = its name, under the team's). */
data class TeamsGroup(val accountId: String, val server: String?, val teams: List<com.exponential.app.data.db.TeamEntity>)

@Composable
fun TeamsScreen(
    onOpenTeam: (teamId: String) -> Unit,
    chats: ChatsViewModel,
    viewModel: TeamsViewModel = hiltViewModel(),
) {
    val groups by viewModel.groups.collectAsStateWithLifecycle()
    val activeId by viewModel.activeAccountId.collectAsStateWithLifecycle()
    val needsYou by chats.needsYouByTeam.collectAsStateWithLifecycle()
    var setup by remember { mutableStateOf(false) }
    Scaffold(containerColor = Color.Transparent) { padding ->
        TeamsContent(
            groups = groups?.let { list -> list.map { TeamsGroup(it.accountId, it.hostname.takeIf { _ -> it.accountId != activeId }, it.teams) } },
            needsYou = needsYou,
            onOpenTeam = { accountId, teamId ->
                viewModel.useAccount(accountId)
                onOpenTeam(teamId)
            },
            onAdd = { setup = true },
            modifier = Modifier.padding(padding),
        )
    }
    if (setup) {
        TeamSetupSheet(
            onDismiss = { setup = false },
            onCreated = { setup = false },
            onJoined = { setup = false },
        )
    }
}

@Composable
fun TeamsContent(
    /** Null = still loading. */
    groups: List<TeamsGroup>?,
    needsYou: Map<String, Int>,
    onOpenTeam: (accountId: String, teamId: String) -> Unit,
    onAdd: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(bottom = BottomBarInset)
            .testTag(TeamsTestIds.TEAMS),
    ) {
        PageHeader(
            title = Chats.Copy.HEADERS.getValue("teams"),
            ownActions = { TeamsAddMenu(onAdd) },
        )
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            when {
                groups == null -> SkeletonRows()
                groups.all { it.teams.isEmpty() } -> EmptyState(
                    EmptyStateCopy.noTeam,
                    action = {
                        GlassPill(
                            "Create or join a team",
                            icon = ExpIcons.uiAdd,
                            size = PillSize.Md,
                            mode = PillMode.Action,
                            onClick = onAdd,
                        )
                    },
                )
                else -> groups.filter { it.teams.isNotEmpty() }.forEach { group ->
                    RowGroup {
                        group.teams.forEach { team ->
                            SettingsRow(
                                icon = ExpIcons.uiTeam,
                                leading = { TeamAvatar(team.iconUrl, size = RowGlyph) },
                                title = team.name,
                                subtitle = group.server,
                                trailing = {
                                    CountBadge(needsYou[team.id] ?: 0, NavBadges.Tone.Attention, testTag = "team-row-badge")
                                },
                                onClick = { onOpenTeam(group.accountId, team.id) },
                                modifier = Modifier.testTag(TeamsTestIds.TEAM_ROW),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** The header's +: `teamsMenu` (New team, Join team: the one create-or-join sheet). */
@Composable
private fun TeamsAddMenu(onPick: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        PageHeaderCircle(ExpIcons.uiAdd, SidebarTree.Copy.NEW_TEAM, onClick = { open = true }, modifier = Modifier.testTag(TeamsTestIds.ADD))
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            ChatMenuPanel(
                groups = listOf(
                    SidebarTree.TEAMS_MENU.map { id ->
                        val join = id == "join-team"
                        ChatMenuItem(
                            label = if (join) SidebarTree.Copy.JOIN_TEAM else SidebarTree.Copy.NEW_TEAM,
                            icon = if (join) ExpIcons.uiInvite else ExpIcons.uiAdd,
                            onClick = onPick,
                            testTag = "teams-menu-$id",
                        )
                    },
                ),
                page = null,
                onPush = {},
                onBack = {},
                onPick = { item ->
                    open = false
                    item.onClick()
                },
            )
        }
    }
}

@Composable
private fun RowGroup(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) { content() }
}

/** TEAM and FOLDER: the same page, a folder has no `…`. */
@Composable
fun TeamPageScreen(
    onBack: () -> Unit,
    onOpenFolder: (teamId: String, folderId: String) -> Unit,
    onOpenBoard: (boardId: String) -> Unit,
    /** The viewer's conversation with the agent, null = none yet. */
    onOpenAgent: (agentId: String, rootSessionId: String?) -> Unit,
    onRunApp: (appId: String) -> Unit,
    /** A new app's draft, in its editor. */
    onOpenApp: (appId: String) -> Unit,
    onOpenChat: (rootSessionId: String) -> Unit,
    onNewAgent: (teamId: String) -> Unit,
    onOpenSettings: (teamId: String) -> Unit,
    chats: ChatsViewModel,
    folder: Boolean = false,
    viewModel: TeamPageViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val agentRows by chats.agentRows.collectAsStateWithLifecycle()
    val list by chats.chatList.collectAsStateWithLifecycle()
    val conversations by chats.conversations.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    var createBoard by remember { mutableStateOf(false) }
    // A gone folder (or a team the viewer left): the page pops.
    val gone = state.loaded && (state.gone || (folder && state.rows == null))
    LaunchedEffect(gone) { if (gone) onBack() }

    val badges = remember(agentRows, list) {
        agentRows.associate { it.agentId to NavBadges.rowBadge(it.unreadCount, it.needsYou) } +
            list.rows.associate { it.sessionId to NavBadges.rowBadge(it.unreadCount, it.needsYou) }
    }
    val marks = remember(list) {
        list.rows.associate { it.sessionId to SessionTree.chatRowMarkState(it.needsYou, it.busy) }
    }
    Scaffold(containerColor = Color.Transparent) { padding ->
        TeamPageContent(
            state = state,
            folder = folder,
            badges = badges,
            marks = marks,
            onBack = onBack,
            onOpenRow = { row ->
                when (row) {
                    is TeamPageRow.Folder -> onOpenFolder(viewModel.teamId, row.id)
                    is TeamPageRow.Board -> onOpenBoard(row.id)
                    is TeamPageRow.Agent -> onOpenAgent(row.id, conversations[row.id])
                    is TeamPageRow.App -> onRunApp(row.id)
                    is TeamPageRow.Session -> onOpenChat(row.id)
                }
            },
            onNew = { kind ->
                when (kind) {
                    SidebarTree.Kind.Board -> createBoard = true
                    SidebarTree.Kind.Agent -> onNewAgent(viewModel.teamId)
                    SidebarTree.Kind.App -> viewModel.createApp(onOpenApp)
                    SidebarTree.Kind.Session -> Unit
                }
            },
            onSettings = { onOpenSettings(viewModel.teamId) },
            modifier = Modifier.padding(padding),
        )
    }
    if (createBoard) {
        CreateBoardSheet(
            teamId = viewModel.teamId,
            onCreated = { createBoard = false },
            onDismiss = { createBoard = false },
        )
    }
}

@Composable
fun TeamPageContent(
    state: TeamPageState,
    folder: Boolean,
    badges: Map<String, NavBadges.Badge>,
    marks: Map<String, SessionTree.SessionState?>,
    onBack: () -> Unit,
    onOpenRow: (TeamPageRow) -> Unit,
    onNew: (SidebarTree.Kind) -> Unit,
    onSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize().testTag(TeamsTestIds.TEAM)) {
        PageHeader(
            title = if (folder) state.folderName.orEmpty() else state.team?.name.orEmpty(),
            onBack = onBack,
            leading = {
                if (folder) SidebarFolderGlyph(size = 24.dp) else TeamAvatar(state.team?.iconUrl, size = 24.dp)
            },
            ownActions = if (folder) null else ({ TeamMoreMenu(state.owner, onNew, onSettings) }),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
        ) {
            val rows = state.rows.orEmpty()
            when {
                !state.loaded -> SkeletonRows()
                rows.isEmpty() -> EmptyState(EmptyStateCopy.boards)
                else -> RowGroup {
                    rows.forEach { row -> TeamItemRow(row, badges[row.id] ?: NavBadges.NONE, marks[row.id], onClick = { onOpenRow(row) }) }
                }
            }
        }
    }
}

@Composable
private fun TeamItemRow(row: TeamPageRow, badge: NavBadges.Badge, mark: SessionTree.SessionState?, onClick: () -> Unit) {
    SettingsRow(
        icon = ExpIcons.uiChevronRight,
        leading = {
            when (row) {
                is TeamPageRow.Folder -> SidebarFolderGlyph(size = RowGlyph)
                is TeamPageRow.Board -> BoardIcon(row.board, size = RowGlyph)
                is TeamPageRow.Agent -> AgentGlyph(icon = row.agent.icon, color = row.agent.color, size = RowGlyph)
                is TeamPageRow.App -> AppGlyph(row.app.icon, row.app.color, size = RowGlyph)
                is TeamPageRow.Session -> AgentRunMark(
                    agent = row.session.cli,
                    state = mark ?: SessionTree.SessionState.Idle,
                    size = 18.dp,
                    badgeSize = 7.dp,
                )
            }
        },
        title = when (row) {
            is TeamPageRow.Folder -> row.name
            is TeamPageRow.Board -> row.board.name
            is TeamPageRow.Agent -> row.agent.name
            is TeamPageRow.App -> row.app.name
            is TeamPageRow.Session -> row.title
        },
        trailing = { CountBadge(badge, testTag = "team-item-badge") },
        onClick = onClick,
        modifier = Modifier.testTag(if (row is TeamPageRow.Folder) TeamsTestIds.FOLDER_ROW else "${TeamsTestIds.ITEM_ROW}-${row.id}"),
    )
}

/** The team page's `…` (`phoneTeamMenu`). */
@Composable
private fun TeamMoreMenu(owner: Boolean, onNew: (SidebarTree.Kind) -> Unit, onSettings: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf<ChatMenuItem?>(null) }
    Box {
        PageHeaderCircle(
            ExpIcons.uiMore,
            SidebarTree.Copy.TEAM_ACTIONS,
            onClick = {
                page = null
                open = true
            },
            modifier = Modifier.testTag(TeamsTestIds.MORE),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            val items = SidebarTree.phoneTeamMenu(owner).map { item ->
                if (item.id == "new") {
                    ChatMenuItem(
                        label = SidebarTree.Copy.NEW,
                        icon = ExpIcons.uiAdd,
                        onClick = {},
                        submenu = item.children.map { kind ->
                            ChatMenuItem(
                                label = SidebarTree.Copy.KINDS.getValue(kind),
                                icon = when (kind) {
                                    SidebarTree.Kind.Board -> ExpIcons.navBoards
                                    SidebarTree.Kind.Agent -> ExpIcons.settingsAgents
                                    SidebarTree.Kind.App -> ExpIcons.navApps
                                    SidebarTree.Kind.Session -> ExpIcons.actionChat
                                },
                                onClick = { onNew(kind) },
                                testTag = "team-menu-new-${kind.wire}",
                            )
                        },
                        testTag = "team-menu-new",
                    )
                } else {
                    ChatMenuItem(
                        label = SidebarTree.Copy.SETTINGS,
                        icon = ExpIcons.navSettings,
                        onClick = onSettings,
                        testTag = "team-menu-settings",
                    )
                }
            }
            ChatMenuPanel(
                groups = listOf(items),
                page = page?.let { p -> items.firstOrNull { it.label == p.label } },
                onPush = { page = it },
                onBack = { page = null },
                onPick = { item ->
                    open = false
                    item.onClick()
                },
            )
        }
    }
}
