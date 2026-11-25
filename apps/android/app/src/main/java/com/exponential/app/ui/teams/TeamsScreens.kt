package com.exponential.app.ui.teams

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SidebarMenus
import com.exponential.app.domain.SidebarTree
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.navigation.TeamsTab
import com.exponential.app.ui.apps.AppGlyph
import com.exponential.app.ui.chat.AgentGlyph
import com.exponential.app.ui.chats.ChatsViewModel
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.CountBadge
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderCircle
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PinnedHeaderColumn
import com.exponential.app.ui.components.SidebarFolderGlyph
import com.exponential.app.ui.components.SidebarFolderTrailing
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.treeGuides
import com.exponential.app.ui.gettingstarted.GettingStartedSection
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.onboarding.TeamSetupSheet
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first

// #EXP-2109 (sidebar-tree.json `phone`): THE TEAMS TAB, ONE page: the member
// teams as the sidebar's nested tree (team › folder › board / agent / app /
// session), one glass group per server. A tap folds a team or a folder and
// opens the rest; a long-press opens the row's menu ([TeamRowMenuSheet]).

private val RowGlyph = 18.dp
private val RowEdge = 16.dp

object TeamsTestIds {
    const val TEAMS = "teams-page"
    const val TEAM_ROW = "team-row"
    const val ITEM_ROW = "team-item-row"
    const val FOLDER_ROW = "team-folder-row"
    const val FOLDER_NAME = "team-folder-name"
    const val TEAM_SETTINGS = "team-settings"
    const val ADD = "teams-add"
}

@Composable
fun TeamsScreen(
    onOpenBoard: (boardId: String) -> Unit,
    /** The viewer's conversation with the agent, null = none yet. */
    onOpenAgent: (agentId: String, rootSessionId: String?) -> Unit,
    onOpenAgentSettings: (agentId: String, rootSessionId: String?) -> Unit,
    onRunApp: (appId: String) -> Unit,
    /** The app's editor, or its settings. */
    onEditApp: (appId: String, settings: Boolean) -> Unit,
    onOpenChat: (rootSessionId: String) -> Unit,
    /** New › Chat: the composer with that team picked. */
    onNewChat: (teamId: String) -> Unit,
    onNewAgent: (teamId: String) -> Unit,
    onNewIssue: (boardId: String) -> Unit,
    /** Null section = the team's settings page. */
    onOpenTeamSettings: (teamId: String, section: String?) -> Unit,
    /** Getting started's "Start a run". */
    onOpenComposer: () -> Unit,
    chats: ChatsViewModel,
    viewModel: TeamsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val needsYou by chats.needsYouByTeam.collectAsStateWithLifecycle()
    val agentRows by chats.agentRows.collectAsStateWithLifecycle()
    val list by chats.chatList.collectAsStateWithLifecycle()
    val conversations by chats.conversations.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val busy by viewModel.archiving.collectAsStateWithLifecycle()
    val haptics = LocalHapticFeedback.current

    var setup by remember { mutableStateOf(false) }
    var createBoard by remember { mutableStateOf<String?>(null) }
    var archiving by remember { mutableStateOf<BoardEntity?>(null) }
    var menuFor by remember { mutableStateOf<TeamTreeRow?>(null) }

    val badges = remember(agentRows, list) {
        agentRows.associate { it.agentId to NavBadges.rowBadge(it.unreadCount, it.needsYou) } +
            list.rows.associate { it.sessionId to NavBadges.rowBadge(it.unreadCount, it.needsYou) }
    }
    val marks = remember(list) {
        list.rows.associate { it.sessionId to SessionTree.chatRowMarkState(it.needsYou, it.busy) }
    }
    val focusManager = LocalFocusManager.current
    val open: (TeamTreeRow) -> Unit = { row ->
        focusManager.clearFocus()
        when (row.target) {
            is TeamTarget.Board -> onOpenBoard(row.row.id)
            is TeamTarget.Agent -> onOpenAgent(row.row.id, conversations[row.row.id])
            is TeamTarget.App -> onRunApp(row.row.id)
            is TeamTarget.Session -> onOpenChat(row.row.id)
            null -> viewModel.toggle(row)
        }
    }
    val actions = TeamMenuActions(
        onOpen = open,
        onNew = { teamId, kind ->
            when (kind) {
                SidebarTree.Kind.Session.wire -> onNewChat(teamId)
                SidebarTree.Kind.Agent.wire -> onNewAgent(teamId)
                SidebarTree.Kind.App.wire -> viewModel.createApp(teamId) { onEditApp(it, false) }
                SidebarTree.Kind.Board.wire -> createBoard = teamId
            }
        },
        onNewIssue = onNewIssue,
        onSendToAgent = chats::sendToAgent,
        onEditApp = onEditApp,
        onAgentSettings = { id -> onOpenAgentSettings(id, conversations[id]) },
        onTeamSettings = onOpenTeamSettings,
        onCopyLink = rememberCopyLink(viewModel),
        onArchive = { archiving = it },
    )

    // A link to a team: it unfolds and the list scrolls to it.
    val landing by TeamsTab.landing.collectAsStateWithLifecycle()
    val scroll = rememberScrollState()
    val tops = remember { mutableStateMapOf<String, Float>() }
    val landable = landing?.takeIf { id -> state.trees.containsKey(id) }
    LaunchedEffect(landable) {
        val teamId = landable ?: return@LaunchedEffect
        viewModel.openTeam(teamId)
        val top = snapshotFlow { tops[teamId] }.filterNotNull().first()
        scroll.animateScrollTo(top.toInt())
        TeamsTab.consume(teamId)
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        PinnedHeaderColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            header = {
                PageHeader(
                    title = Chats.Copy.HEADERS.getValue("teams"),
                    ownActions = { TeamsAddMenu { setup = true } },
                )
            },
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    // A tap beside the rows ends a name in edit.
                    .pointerInput(Unit) { detectTapGestures { focusManager.clearFocus() } }
                    .imePadding()
                    .verticalScroll(scroll)
                    .padding(bottom = BottomBarInset)
                    .padding(horizontal = 16.dp, vertical = 8.dp)
                    .testTag(TeamsTestIds.TEAMS),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                when {
                    !state.loaded -> SkeletonRows()
                    state.rows.isEmpty() && state.others.isEmpty() -> EmptyState(
                        EmptyStateCopy.noTeam,
                        action = {
                            GlassPill(
                                "Create or join a team",
                                icon = ExpIcons.uiAdd,
                                size = PillSize.Md,
                                mode = PillMode.Action,
                                onClick = { setup = true },
                            )
                        },
                    )
                    state.rows.isNotEmpty() -> TeamsTree(
                        state = state,
                        needsYou = needsYou,
                        badges = badges,
                        marks = marks,
                        onOpen = open,
                        onMenu = { row ->
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            menuFor = row
                        },
                        onRename = { row, name -> viewModel.rename(row.team.id, row.row.id, name) },
                        newMenu = { row -> listOf(newItems(row.team.id, row.team.id in state.owned, actions)) },
                        moreMenu = { row -> teamRowMenu(row, state, viewModel, actions, header = true) },
                        onTeamTop = { id, top -> tops[id] = top },
                    )
                }
                state.emptyTeamId?.let { teamId ->
                    // Nothing yet: a board is the first step; the member's progress.
                    Column {
                        EmptyState(
                            EmptyStateCopy.boards,
                            action = if (teamId in state.owned) {
                                {
                                    GlassPill(
                                        EmptyStateCopy.boards.action!!,
                                        icon = ExpIcons.uiAdd,
                                        size = PillSize.Md,
                                        mode = PillMode.Action,
                                        onClick = { createBoard = teamId },
                                    )
                                }
                            } else {
                                null
                            },
                        )
                        GettingStartedSection(onOpenAgent = onOpenComposer, modifier = Modifier.padding(top = 20.dp))
                    }
                }
                // The other signed-in servers' teams: flat, each activates its server.
                state.others.forEach { group ->
                    OtherServerTeams(group) { team ->
                        viewModel.useAccount(group.accountId)
                        TeamsTab.land(team.id)
                    }
                }
            }
        }
    }

    menuFor?.let { row ->
        TeamRowMenuSheet(
            title = row.name,
            groups = teamRowMenu(row, state, viewModel, actions),
            onDismiss = { menuFor = null },
        )
    }
    if (setup) {
        TeamSetupSheet(
            onDismiss = { setup = false },
            onCreated = { setup = false },
            onJoined = { setup = false },
        )
    }
    createBoard?.let { teamId ->
        CreateBoardSheet(
            teamId = teamId,
            onCreated = { createBoard = null },
            onDismiss = { createBoard = null },
        )
    }
    archiving?.let { board ->
        ArchiveBoardConfirm(
            board,
            busy = busy,
            onConfirm = { viewModel.archive(board) { archiving = null } },
            onDismiss = { archiving = null },
        )
    }
}

/** The active account's tree: one glass group, no hairlines, tree guides. */
@Composable
private fun TeamsTree(
    state: TeamsState,
    needsYou: Map<String, Int>,
    badges: Map<String, NavBadges.Badge>,
    marks: Map<String, SessionTree.SessionState?>,
    onOpen: (TeamTreeRow) -> Unit,
    onMenu: (TeamTreeRow) -> Unit,
    onRename: (TeamTreeRow, String) -> Unit,
    newMenu: (TeamTreeRow) -> List<List<ChatMenuItem>>,
    moreMenu: (TeamTreeRow) -> List<List<ChatMenuItem>>,
    onTeamTop: (teamId: String, top: Float) -> Unit,
) {
    val guides = remember(state.rows) { TreeGuides.compute(state.rows.map { it.row.depth }) }
    Column(Modifier.fillMaxWidth().glassGroup()) {
        state.rows.forEachIndexed { index, row ->
            androidx.compose.runtime.key(row.row.kind, row.row.id) {
                TeamTreeRowView(
                    row = row,
                    guide = guides.getOrNull(index),
                    editing = state.editingId == row.row.id,
                    onClick = { onOpen(row) },
                    onMenu = { onMenu(row) },
                    onRename = { onRename(row, it) },
                    modifier = if (row.row.kind == SidebarTree.ROW_TEAM) {
                        Modifier.onGloballyPositioned { onTeamTop(row.team.id, it.positionInParent().y) }
                    } else {
                        Modifier
                    },
                    trailing = {
                        when {
                            row.row.kind == SidebarTree.ROW_TEAM -> {
                                CountBadge(needsYou[row.team.id] ?: 0, NavBadges.Tone.Attention, testTag = "team-row-badge")
                                // header-actions.json `rows.team`: + then `…`.
                                MenuAnchor(groups = { newMenu(row) }) { open ->
                                    CircleIconButton(
                                        ExpIcons.uiAdd,
                                        "${SidebarMenus.COPY.getValue("new")}, ${row.name}",
                                        onClick = open,
                                        borderless = true,
                                        glyphSize = 16.dp,
                                    )
                                }
                                MenuAnchor(groups = { moreMenu(row) }) { open ->
                                    CircleIconButton(
                                        ExpIcons.uiMore,
                                        "${SidebarTree.Copy.TEAM_ACTIONS}, ${row.name}",
                                        onClick = open,
                                        borderless = true,
                                        glyphSize = 16.dp,
                                    )
                                }
                                FoldChevron(row, showCount = false)
                            }
                            row.group -> FoldChevron(row, showCount = true)
                            else -> CountBadge(badges[row.row.id] ?: NavBadges.NONE, testTag = "team-item-badge")
                        }
                    },
                    mark = marks[row.row.id],
                )
            }
        }
    }
}

/** The fold state is the ROW's ([stateDescription]), never the glyph's. */
@Composable
private fun FoldChevron(row: TeamTreeRow, showCount: Boolean) {
    Box(Modifier.clearAndSetSemantics {}) {
        SidebarFolderTrailing(open = row.row.open, count = row.row.count, showCount = showCount)
    }
}

/** One row: the glyph 18 at 16 + 14 per depth, the name, its trailing; 48dp
 *  tall or taller (large text grows it). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TeamTreeRowView(
    row: TeamTreeRow,
    guide: TreeGuide?,
    editing: Boolean,
    onClick: () -> Unit,
    onMenu: () -> Unit,
    onRename: (String) -> Unit,
    mark: SessionTree.SessionState?,
    modifier: Modifier = Modifier,
    trailing: @Composable () -> Unit,
) {
    val group = row.group
    val open = row.row.open
    val tag = when (row.row.kind) {
        SidebarTree.ROW_TEAM -> TeamsTestIds.TEAM_ROW
        SidebarTree.ROW_FOLDER -> TeamsTestIds.FOLDER_ROW
        else -> "${TeamsTestIds.ITEM_ROW}-${row.row.id}"
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .then(
                if (editing) {
                    Modifier
                } else {
                    Modifier
                        .combinedClickable(
                            role = Role.Button,
                            onClickLabel = if (!group) SidebarMenus.COPY.getValue("open") else if (open) "Collapse" else "Expand",
                            onLongClickLabel = "Actions",
                            onLongClick = onMenu,
                            onClick = onClick,
                        )
                        .semantics {
                            if (group) {
                                stateDescription = if (open) "Expanded" else "Collapsed, ${row.row.count}"
                                if (open) {
                                    collapse {
                                        onClick()
                                        true
                                    }
                                } else {
                                    expand {
                                        onClick()
                                        true
                                    }
                                }
                            }
                        }
                },
            )
            .testTag(tag)
            // A gutter's centre = the parent glyph's centre.
            .treeGuides(guide, base = RowEdge + RowGlyph / 2 - (TreeGuides.INDENT_DP / 2).dp)
            .padding(start = RowEdge + (TreeGuides.INDENT_DP * row.row.depth).dp, end = RowEdge),
    ) {
        Box(Modifier.size(RowGlyph), contentAlignment = Alignment.Center) { RowLead(row, mark) }
        Spacer(Modifier.width(12.dp))
        if (editing) {
            NameField(row.name, onDone = onRename, modifier = Modifier.weight(1f).padding(vertical = 8.dp))
        } else {
            Text(
                row.name,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(vertical = 8.dp),
            )
        }
        Spacer(Modifier.width(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) { trailing() }
    }
}

@Composable
private fun RowLead(row: TeamTreeRow, mark: SessionTree.SessionState?) {
    when (val target = row.target) {
        is TeamTarget.Board -> BoardIcon(target.board, size = RowGlyph)
        is TeamTarget.Agent -> AgentGlyph(icon = target.agent.icon, color = target.agent.color, size = RowGlyph)
        is TeamTarget.App -> AppGlyph(target.app.icon, target.app.color, size = RowGlyph)
        is TeamTarget.Session -> AgentRunMark(
            agent = target.session.cli,
            state = mark ?: SessionTree.SessionState.Idle,
            size = 16.dp,
            badgeSize = 7.dp,
        )
        null -> if (row.row.kind == SidebarTree.ROW_TEAM) {
            TeamAvatar(row.team.iconUrl, size = RowGlyph)
        } else {
            SidebarFolderGlyph(size = RowGlyph)
        }
    }
}

/** A folder's name in edit, in place: Done, a tap elsewhere or Back keeps it. */
@Composable
private fun NameField(name: String, onDone: (String) -> Unit, modifier: Modifier = Modifier) {
    var value by remember { mutableStateOf(TextFieldValue(name, TextRange(0, name.length))) }
    val focus = remember { FocusRequester() }
    var focused by remember { mutableStateOf(false) }
    var done by remember { mutableStateOf(false) }
    fun commit() {
        if (done) return
        done = true
        onDone(value.text)
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    // Leaving the tab keeps the typed name.
    DisposableEffect(Unit) { onDispose { commit() } }
    BackHandler { commit() }
    BasicTextField(
        value = value,
        onValueChange = { value = it },
        singleLine = true,
        textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
        cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { commit() }),
        modifier = modifier
            .focusRequester(focus)
            .onFocusChanged {
                if (it.isFocused) focused = true else if (focused) commit()
            }
            .testTag(TeamsTestIds.FOLDER_NAME),
    )
}

/** Another server's teams: flat rows naming it. */
@Composable
private fun OtherServerTeams(group: TeamsGroup, onOpen: (TeamEntity) -> Unit) {
    Column(Modifier.fillMaxWidth().glassGroup()) {
        group.teams.forEach { team ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .clickable(role = Role.Button) { onOpen(team) }
                    .testTag(TeamsTestIds.TEAM_ROW)
                    .padding(horizontal = RowEdge, vertical = 8.dp),
            ) {
                TeamAvatar(team.iconUrl, size = RowGlyph)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        team.name,
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        group.server,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

/** The header's +: `teamsMenu` (New team, Join team: the one create-or-join sheet). */
@Composable
private fun TeamsAddMenu(onPick: () -> Unit) {
    MenuAnchor(
        groups = {
            listOf(
                SidebarTree.TEAMS_MENU.map { id ->
                    menuItem("teams-$id", SidebarMenus.COPY.getValue(id), SidebarMenus.ICONS.getValue(id), onClick = onPick)
                },
            )
        },
    ) { open ->
        PageHeaderCircle(ExpIcons.uiAdd, SidebarTree.Copy.NEW_TEAM, onClick = open, modifier = Modifier.testTag(TeamsTestIds.ADD))
    }
}
