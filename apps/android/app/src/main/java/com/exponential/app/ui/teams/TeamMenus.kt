package com.exponential.app.ui.teams

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.HeaderActions
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.domain.SidebarMenus
import com.exponential.app.domain.SidebarTree
import com.exponential.app.ui.chat.ChatMenuPanel
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.ChatMenuSheet
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PageHeaderCircle
import com.exponential.app.ui.icons.ExpIcons

// #EXP-2109 (sidebar-menus.json `phone`, header-actions.json): the menus of
// the Teams tab's rows and of a board's header, off THE one table.

/** What a row's verbs do outside the layout. */
class TeamMenuActions(
    val onOpen: (TeamTreeRow) -> Unit,
    /** THE New list's pick ([SidebarMenus.NEW_KINDS]) in a team. */
    val onNew: (teamId: String, kind: String) -> Unit,
    val onNewIssue: (boardId: String) -> Unit,
    val onSendToAgent: (agentId: String, text: String) -> Unit,
    val onEditApp: (appId: String, settings: Boolean) -> Unit,
    val onAgentSettings: (agentId: String) -> Unit,
    /** Null section = the team's settings page. */
    val onTeamSettings: (teamId: String, section: String?) -> Unit,
    val onCopyLink: (TeamTreeRow) -> Unit,
    val onArchive: (BoardEntity) -> Unit,
)

/** One verb's row. */
internal fun menuItem(id: String, label: String, concept: String, destructive: Boolean = false, submenu: List<ChatMenuItem>? = null, onClick: () -> Unit = {}) =
    ChatMenuItem(
        label = label,
        icon = ExpIcons.byConcept(concept) ?: ExpIcons.uiMore,
        onClick = onClick,
        destructive = destructive,
        submenu = submenu,
        testTag = "team-menu-$id",
    )

/** THE New list as menu rows. */
internal fun newItems(teamId: String, owner: Boolean, actions: TeamMenuActions): List<ChatMenuItem> =
    SidebarMenus.newKinds(owner).map { kind ->
        menuItem("new-$kind", SidebarMenus.NEW_COPY.getValue(kind), SidebarMenus.NEW_ICONS.getValue(kind)) { actions.onNew(teamId, kind) }
    }

/** A row's menu in groups: [SidebarMenus.phoneMenuGroups] with its verbs;
 *  [header] = a header's or a row's `…` (what the + does is not in it). */
internal fun teamRowMenu(
    row: TeamTreeRow,
    state: TeamsState,
    viewModel: TeamsViewModel,
    actions: TeamMenuActions,
    header: Boolean = false,
): List<List<ChatMenuItem>> {
    val teamId = row.team.id
    val id = row.row.id
    val kind = row.row.kind
    val owner = teamId in state.owned
    fun item(verb: String): ChatMenuItem? {
        fun entry(submenu: List<ChatMenuItem>? = null, onClick: () -> Unit = {}) = menuItem(
            id = verb,
            label = SidebarMenus.COPY.getValue(verb),
            concept = SidebarMenus.ICONS.getValue(verb),
            destructive = SidebarMenus.destructive(kind, verb),
            submenu = submenu,
            onClick = onClick,
        )
        val target = row.target
        return when (verb) {
            "open" -> entry { actions.onOpen(row) }
            "new" -> entry(newItems(teamId, owner, actions))
            "new-folder" -> entry { viewModel.addFolder(teamId) }
            "rename" -> entry { viewModel.edit(id) }
            "ungroup" -> entry { viewModel.ungroup(teamId, id) }
            "new-issue" -> entry { actions.onNewIssue(id) }
            "compact" -> entry { SessionScreen.MenuEntry.Compact.send?.let { actions.onSendToAgent(id, it) } }
            "new-conversation" -> entry { SessionScreen.MenuEntry.NewConversation.send?.let { actions.onSendToAgent(id, it) } }
            "edit" -> entry { actions.onEditApp(id, false) }
            "copy-link" -> entry { actions.onCopyLink(row) }
            "move" -> entry(moveItems(row, state, viewModel))
            "remove-from-sidebar" -> entry { viewModel.removeFromSidebar(teamId, id) }
            "settings" -> entry {
                when (target) {
                    is TeamTarget.Board -> actions.onTeamSettings(teamId, SettingsNav.BOARDS)
                    is TeamTarget.App -> actions.onEditApp(id, true)
                    is TeamTarget.Agent -> actions.onAgentSettings(id)
                    else -> actions.onTeamSettings(teamId, null)
                }
            }
            "archive" -> (target as? TeamTarget.Board)?.let { entry { actions.onArchive(it.board) } }
            else -> null
        }
    }
    val input = SidebarMenus.Input(owner = owner)
    val verbs = SidebarMenus.phoneMenuGroups(kind, input)
        .map { group -> if (header) group.filter { it !in HeaderActions.MENU_DROPS } else group }
    return verbs.map { group -> group.mapNotNull(::item) }.filter { it.isNotEmpty() }
}

/** Move to folder ›: the team's folders, New folder, (inside one) Top level. */
private fun moveItems(row: TeamTreeRow, state: TeamsState, viewModel: TeamsViewModel): List<ChatMenuItem> {
    val targets = state.moveTargets(row)
    val teamId = row.team.id
    val id = row.row.id
    val folders = targets.folders.map { folder ->
        menuItem("move-${folder.id}", folder.name ?: SidebarFolders.DEFAULT_FOLDER_NAME, SidebarMenus.ICONS.getValue("move")) {
            viewModel.moveInto(teamId, id, folder.id)
        }
    }
    val create = menuItem("move-new-folder", SidebarMenus.COPY.getValue("new-folder"), SidebarMenus.ICONS.getValue("new-folder")) {
        viewModel.moveToNewFolder(teamId, id)
    }.copy(dividerBefore = folders.isNotEmpty())
    val out = targets.topLevel?.let { holder ->
        menuItem("move-top-level", SidebarMenus.COPY.getValue("top-level"), "ui-minus") { viewModel.moveOut(teamId, id, holder) }
    }
    return folders + listOfNotNull(create, out)
}

/** A long-press' menu: the chat menu's sheet titled by the row; a submenu
 *  entry opens its rows as the next sheet. */
@Composable
internal fun TeamRowMenuSheet(title: String, groups: List<List<ChatMenuItem>>, onDismiss: () -> Unit) {
    var page by remember { mutableStateOf<ChatMenuItem?>(null) }
    val rows = page?.submenu ?: groups.flatMapIndexed { index, group ->
        group.mapIndexed { i, item -> if (index > 0 && i == 0) item.copy(dividerBefore = true) else item }
    }
    ChatMenuSheet(
        title = page?.label ?: title,
        items = rows.map { item ->
            if (item.submenu != null) {
                item.copy(chevron = true, onClick = { page = item })
            } else {
                item.copy(onClick = {
                    onDismiss()
                    item.onClick()
                })
            }
        },
        onDismiss = onDismiss,
        testTag = "team-row-menu",
    )
}

/** A button's dropdown over [groups] (submenus push their page). */
@Composable
internal fun MenuAnchor(
    groups: () -> List<List<ChatMenuItem>>,
    anchor: @Composable (open: () -> Unit) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf<ChatMenuItem?>(null) }
    Box {
        anchor {
            page = null
            open = true
        }
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            val rows = groups()
            ChatMenuPanel(
                groups = rows,
                page = page?.let { p -> rows.flatten().firstOrNull { it.testTag == p.testTag } },
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

/** Copy link: the row's web page on the clipboard, `Link copied`. */
@Composable
internal fun rememberCopyLink(viewModel: TeamsViewModel): (TeamTreeRow) -> Unit {
    val clipboard = LocalClipboardManager.current
    val toaster = LocalToaster.current
    return { row ->
        viewModel.link(row)?.let { url ->
            clipboard.setText(AnnotatedString(url))
            toaster.success(SidebarMenus.COPY.getValue("linkCopied"))
        }
    }
}

/** A board's Archive asks first (web `WEB_PROMPTS.archiveBoard`). */
@Composable
internal fun ArchiveBoardConfirm(board: BoardEntity, busy: Boolean, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    val dismiss = { if (!busy) onDismiss() }
    GlassAlert(
        title = "Archive \"${board.name}\"?",
        body = "It and its issues disappear for the whole team. An owner can bring it back in team settings.",
        onDismiss = dismiss,
        trailing = listOf(
            GlassAlertAction("Cancel", onClick = dismiss),
            GlassAlertAction(
                SidebarMenus.COPY.getValue("archive"),
                primary = true,
                loading = busy,
                testTag = "board-archive-confirm",
                onClick = onConfirm,
            ),
        ),
        defaultAction = 1,
    )
}

/** header-actions.json `board`: + (New issue) and `…` (the board's menu without
 *  what the + does). */
@Composable
fun BoardHeaderActions(
    boardId: String,
    onNewIssue: () -> Unit,
    onOpenTeamSettings: (teamId: String, section: String) -> Unit,
    /** The board is archived: its page leaves. */
    onArchived: () -> Unit,
    viewModel: TeamsViewModel = hiltViewModel(key = "board-header"),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val busy by viewModel.archiving.collectAsStateWithLifecycle()
    var archiving by remember { mutableStateOf<BoardEntity?>(null) }
    val copyLink = rememberCopyLink(viewModel)
    PageHeaderCircle(
        ExpIcons.uiAdd,
        SidebarMenus.COPY.getValue("new-issue"),
        onClick = onNewIssue,
        modifier = Modifier.testTag("board-add"),
    )
    val row = state.leaves[boardId]
    if (row != null) {
        val actions = TeamMenuActions(
            onOpen = {},
            onNew = { _, _ -> },
            onNewIssue = {},
            onSendToAgent = { _, _ -> },
            onEditApp = { _, _ -> },
            onAgentSettings = {},
            onTeamSettings = { teamId, section -> onOpenTeamSettings(teamId, section ?: SettingsNav.BOARDS) },
            onCopyLink = copyLink,
            onArchive = { archiving = it },
        )
        MenuAnchor(groups = { teamRowMenu(row, state, viewModel, actions, header = true) }) { open ->
            PageHeaderCircle(ExpIcons.uiMore, "Board actions", onClick = open, modifier = Modifier.testTag("board-more"))
        }
    }
    archiving?.let { board ->
        ArchiveBoardConfirm(
            board,
            busy = busy,
            onConfirm = {
                viewModel.archive(board) { archived ->
                    archiving = null
                    if (archived) onArchived()
                }
            },
            onDismiss = { archiving = null },
        )
    }
}
