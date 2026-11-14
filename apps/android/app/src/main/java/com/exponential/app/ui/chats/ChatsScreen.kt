package com.exponential.app.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionScreen
import com.exponential.app.ui.chat.sessionScreenMenuItems
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.chats.NeedsYouEntryRow
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.AgentWorkingMark
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.ChatDeleteConfirm
import com.exponential.app.ui.components.ChatArchivedRow
import com.exponential.app.ui.components.ChatMenuSheet
import com.exponential.app.ui.components.ChatRow
import com.exponential.app.ui.components.AgentChatRowItem
import com.exponential.app.ui.components.AppChatRowItem
import com.exponential.app.ui.components.ChatSectionHeader
import com.exponential.app.ui.components.ChatSelectionBar
import com.exponential.app.ui.components.SelectionBarSlot
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import com.exponential.app.ui.components.BottomNavDefaults
import com.exponential.app.ui.components.LocalBottomBarSuppression
import com.exponential.app.domain.DomainContract
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.DisposableEffect
import androidx.compose.foundation.layout.navigationBarsPadding
import com.exponential.app.ui.components.LocalPageHeaderActions
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.chatMenuItems
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * Session 58cb32df: the CHATS tab (chats.json LIST, phones). The large
 * in-content headline `copy.title` with its search + inbox circles, then
 * #FEED-191 the Needs you entry (needs-you.json, none = no row), the Agents
 * section, #EXP-1733 the Apps section (a tap opens the app's editor) and the
 * active team's chats grouped ([Chats.chatSections] over
 * [Chats.chatList]: Needs you · Working · Unread · Chats, no bands, no nesting,
 * each newest activity first). A row opens its chat. #EXP-1388: a long-press
 * ENTERS THE SELECTION with that row (taps toggle, nothing opens; the bar = the
 * issue list's bulk bar in the tab bar's slot: Mark read / unread, Archive /
 * Unarchive, Delete; S-3: no Stop); the Archived entry (first under the Chats
 * header) pushes the Archived page ([view] Archived: the page header with back,
 * the archived rows flat). An agent row opens the agent's chat; its long-press
 * the agent menu.
 */
@Composable
fun ChatsScreen(
    onOpenChat: (rootSessionId: String) -> Unit,
    /** EXP-1323: kept for the route's signature (the phone list has no child rows). */
    onOpenSession: (sessionId: String) -> Unit = onOpenChat,
    /** NEW-7: an agent with no conversation of mine yet = its empty DM. */
    onOpenAgent: (agentId: String) -> Unit = {},
    /** #EXP-1733: an app row opens the app's chat (its editor). */
    onOpenApp: (appId: String) -> Unit = {},
    /** chats.json: the header's inbox circle pushes the inbox-only page. */
    onOpenInbox: () -> Unit = {},
    /** #FEED-239: an agent row's `…` › Agent settings = that agent's chat
     *  ([rootSessionId], null = its empty DM) with its settings open. */
    onOpenAgentSettings: (agentId: String, rootSessionId: String?) -> Unit = { _, _ -> },
    /** The inbox circle's dot: any notification unread. */
    inboxUnread: Int = 0,
    /** #EXP-1388: the list, or the pushed Archived page. */
    view: Chats.ChatListView = Chats.ChatListView.List,
    /** #EXP-1388: the Archived entry pushes the Archived page. */
    onOpenArchived: () -> Unit = {},
    /** #FEED-191: the Needs you entry pushes THE list. */
    onOpenNeedsYou: () -> Unit = {},
    /** #EXP-1388: the Archived page's back control. */
    onBack: () -> Unit = {},
    viewModel: ChatsViewModel = hiltViewModel(),
) {
    LaunchedEffect(view) { viewModel.setView(view) }
    val list by viewModel.chatList.collectAsStateWithLifecycle()
    val agentRows by viewModel.agentRows.collectAsStateWithLifecycle()
    val appRows by viewModel.appRows.collectAsStateWithLifecycle()
    val agents by viewModel.agents.collectAsStateWithLifecycle()
    val needsYouRows by viewModel.needsYouRows.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { viewModel.ensureAssistant() }
    val facts by viewModel.rootFacts.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val toaster = LocalToaster.current
    LaunchedEffect(notice) {
        val text = notice ?: return@LaunchedEffect
        toaster.warning(text)
        viewModel.clearNotice()
    }

    val haptics = LocalHapticFeedback.current
    var agentMenuFor by remember { mutableStateOf<SessionTree.AgentRow?>(null) }
    var agentMenuSession by remember { mutableStateOf<String?>(null) }
    var deleteFor by remember { mutableStateOf<List<String>?>(null) }

    // #EXP-1388: the selection; it keeps only rows still in the list, and
    // leaving it (empty, close, back) hands the tab bar's
    // slot back.
    var selected by remember { mutableStateOf(emptyList<String>()) }
    val rowIds = list.rows.map { it.sessionId }
    LaunchedEffect(rowIds) {
        val kept = selected.filter { it in rowIds }
        if (kept.size != selected.size) selected = kept
    }
    val selecting = selected.isNotEmpty()
    val barSuppression = LocalBottomBarSuppression.current
    DisposableEffect(selecting) {
        barSuppression?.suppressed = selecting
        onDispose { barSuppression?.suppressed = false }
    }
    BackHandler(enabled = selecting) { selected = emptyList() }
    val actions = remember(selected, list, facts, isOwner, view) {
        // #FEED-191 (chats.json): selection order = the displayed order.
        Chats.chatSelectionActions(
            chatDisplayedRows(list.rows, view).filter { it.sessionId in selected }.map { row ->
                val id = row.sessionId
                val root = facts[id]
                Chats.ChatSelectionRow(
                    sessionId = id,
                    rootStatus = root?.status ?: DomainContract.sessionStatusEnded,
                    unreadCount = row.unreadCount,
                    canSteer = root?.canSteer == true,
                    canDelete = root != null && (root.mine || isOwner),
                )
            },
            view,
        )
    }

    Box(Modifier.fillMaxSize()) {
        Scaffold(containerColor = Color.Transparent) { padding ->
            ChatsContent(
                list = list,
                agentOf = { facts[it]?.agent },
                needsYouCount = needsYouRows.size,
                onOpenNeedsYou = onOpenNeedsYou,
                onOpenChat = onOpenChat,
                onLongPressChat = { row ->
                    // The issue list's entry: the haptic, then the one row.
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    selected = listOf(row.sessionId)
                },
                onLongPressAgent = { row ->
                    agents.firstOrNull { it.agentId == row.agentId }?.let { agent ->
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        agentMenuFor = agent
                        agentMenuSession = row.sessionId
                    }
                },
                agentRows = agentRows,
                onOpenAgentRow = { row -> row.sessionId?.let(onOpenChat) ?: onOpenAgent(row.agentId) },
                appRows = appRows,
                onOpenAppRow = { row -> onOpenApp(row.appId) },
                onOpenInbox = onOpenInbox,
                inboxUnread = inboxUnread,
                view = view,
                onOpenArchived = onOpenArchived,
                onBack = onBack,
                selected = selected,
                onToggleSelect = { id -> selected = if (id in selected) selected - id else selected + id },
                modifier = Modifier.padding(padding),
            )
        }
        if (selecting) {
            // THE issue list's slot (this Box sits outside the Scaffold, so it
            // adds the nav-bar inset the issue list's Scaffold applied).
            SelectionBarSlot(Modifier.navigationBarsPadding()) {
            ChatSelectionBar(
                count = selected.size,
                actions = actions,
                onClear = { selected = emptyList() },
                onAction = { entry ->
                    val ids = entry.sessionIds
                    when (entry.action) {
                        Chats.ChatSelectionAction.MarkRead -> viewModel.setRead(ids, read = true)
                        Chats.ChatSelectionAction.MarkUnread -> viewModel.setRead(ids, read = false)
                        Chats.ChatSelectionAction.Archive -> viewModel.setArchived(ids, archived = true)
                        Chats.ChatSelectionAction.Unarchive -> viewModel.setArchived(ids, archived = false)
                        Chats.ChatSelectionAction.Delete -> deleteFor = ids
                    }
                    // Every action leaves the selection.
                    selected = emptyList()
                },
            )
            }
        }
    }

    agentMenuFor?.let { agent ->
        ChatMenuSheet(
            title = agent.name,
            // #FEED-239: THE session screen `…` (kind agent / assistant).
            items = run {
                val kind = if (agent.assistant) SessionScreen.Kind.Assistant else SessionScreen.Kind.Agent
                sessionScreenMenuItems(
                    kind,
                    SessionScreen.menu(
                        kind,
                        phone = false,
                        member = true,
                        settings = agent.assistant || isOwner,
                        publish = false,
                        export = false,
                        delete = false,
                    ),
                ) { entry ->
                    agentMenuFor = null
                    when (entry) {
                        SessionScreen.MenuEntry.Compact, SessionScreen.MenuEntry.NewConversation ->
                            entry.send?.let { viewModel.sendToAgent(agent.agentId, it) }
                        SessionScreen.MenuEntry.Settings -> onOpenAgentSettings(agent.agentId, agentMenuSession)
                        else -> Unit
                    }
                }
            },
            onDismiss = { agentMenuFor = null },
        )
    }
    deleteFor?.let { rootIds ->
        ChatDeleteConfirm(
            title = Chats.chatDeleteTitle(rootIds.size),
            onConfirm = {
                deleteFor = null
                viewModel.deleteChats(rootIds)
            },
            onDismiss = { deleteFor = null },
        )
    }
}

/** #FEED-191 (chats.json): the rows in the order they show: the sections',
 *  or the Archived page's flat rows. */
internal fun chatDisplayedRows(rows: List<Chats.ChatRow>, view: Chats.ChatListView): List<Chats.ChatRow> =
    if (view == Chats.ChatListView.Archived) rows else Chats.chatSections(rows).flatMap { it.rows }

/** The Chats tab's stateless body (the styleguide's `chat-list` and
 *  `chat-selection` draw it). */
@Composable
fun ChatsContent(
    list: Chats.ChatList,
    agentOf: (rootId: String) -> String?,
    onOpenChat: (rootSessionId: String) -> Unit,
    onLongPressChat: (Chats.ChatRow) -> Unit,
    onLongPressAgent: (Chats.AgentChatRow) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Int,
    modifier: Modifier = Modifier,
    /** #EXP-1388: the list, or the Archived page. */
    view: Chats.ChatListView = Chats.ChatListView.List,
    onOpenArchived: () -> Unit = {},
    onBack: () -> Unit = {},
    /** #EXP-1388: the selection (empty = not selecting). */
    selected: List<String> = emptyList(),
    onToggleSelect: (rootSessionId: String) -> Unit = {},
    /** S-3: the AGENT rows pinned on top (Assistant first); a tap opens the
     *  agent's chat. */
    agentRows: List<Chats.AgentChatRow> = emptyList(),
    onOpenAgentRow: (Chats.AgentChatRow) -> Unit = {},
    /** #EXP-1733: the APP rows between the agents and the sessions (never
     *  selectable); a tap opens the app's chat (its editor). */
    appRows: List<Chats.AppChatRow> = emptyList(),
    onOpenAppRow: (Chats.AppChatRow) -> Unit = {},
    /** #FEED-191 (needs-you.json): the entry's count; 0 = no entry. */
    needsYouCount: Int = 0,
    onOpenNeedsYou: () -> Unit = {},
) {
    // #EXP-1384: the header's search pushes the phone search; the inbox
    // circle is this list's own (the specimen drives it).
    val shell = LocalPageHeaderActions.current
    val headerActions = PageHeaderActions(
        onSearch = { shell?.onSearch?.invoke() },
        onInbox = onOpenInbox,
        inboxUnread = inboxUnread,
    )
    val archivedPage = view == Chats.ChatListView.Archived
    val selecting = selected.isNotEmpty()
    val rows = list.rows
    // #EXP-1388: the Archived entry stays while chats are archived: the first
    // row under the `chats` header (the header shows while the entry does).
    val archivedEntry = !archivedPage && list.archived > 0
    val sections = remember(rows, archivedPage) {
        if (archivedPage) emptyList() else Chats.chatSections(rows)
    }
    val chatRowItem: @Composable (Chats.ChatRow) -> Unit = { row ->
        ChatRow(
            row = row,
            agent = agentOf(row.sessionId),
            onClick = { if (selecting) onToggleSelect(row.sessionId) else onOpenChat(row.sessionId) },
            onLongClick = { if (selecting) onToggleSelect(row.sessionId) else onLongPressChat(row) },
            selected = if (selecting) row.sessionId in selected else null,
        )
    }
    LazyColumn(
        modifier = modifier.fillMaxSize().testTag(SessionTree.ListCopy.TEST_ID_LIST),
        contentPadding = PaddingValues(top = 4.dp, bottom = BottomBarInset),
    ) {
        item(key = "__header__") {
            if (archivedPage) {
                PageHeader(title = Chats.Copy.ARCHIVED_TITLE, onBack = onBack, actions = headerActions)
            } else {
                PageHeader(title = Chats.Copy.HEADERS.getValue("chats"), actions = headerActions)
            }
        }
        // #FEED-191 (needs-you.json THE ENTRY): the first row; none = no row.
        if (!archivedPage && needsYouCount > 0) {
            item(key = "__needs_you__") {
                NeedsYouEntryRow(count = needsYouCount, onClick = if (selecting) null else onOpenNeedsYou)
            }
        }
        // S-3: the pinned AGENTS section, its header only while an agent row
        // exists.
        if (!archivedPage && agentRows.isNotEmpty()) {
            item(key = "__section_agents__") { ChatSectionHeader(Chats.Copy.SECTION_AGENTS) }
            items(agentRows, key = { row -> "agent_${row.agentId}" }) { row ->
                AgentChatRowItem(
                    row = row,
                    onClick = { if (!selecting) onOpenAgentRow(row) },
                    onLongClick = { if (!selecting) onLongPressAgent(row) },
                )
            }
        }
        // #EXP-1733: the APPS section between the agents and the sessions;
        // none on the Archived page.
        if (!archivedPage && appRows.isNotEmpty()) {
            item(key = "__section_apps__") { ChatSectionHeader(Chats.Copy.SECTION_APPS) }
            items(appRows, key = { row -> "app_${row.appId}" }) { row ->
                AppChatRowItem(
                    row = row,
                    onClick = { if (!selecting) onOpenAppRow(row) },
                )
            }
        }
        if (rows.isEmpty() && !archivedEntry && (archivedPage || (agentRows.isEmpty() && appRows.isEmpty()))) {
            item(key = "__empty__") {
                Box(Modifier.fillMaxWidth().height(320.dp)) {
                    EmptyState(if (archivedPage) EmptyStateCopy.chats.copy(id = "archived", iconConcept = "ui-archive", title = Chats.Copy.ARCHIVED_EMPTY) else EmptyStateCopy.chats)
                }
            }
        }
        if (archivedPage) {
            items(rows, key = { row -> "chat_${row.sessionId}" }) { row -> chatRowItem(row) }
        } else {
            sections.forEach { section ->
                item(key = "__section_${section.key.wire}__") { ChatSectionHeader(section.key.title) }
                if (section.key == Chats.ChatSectionKey.Chats && archivedEntry) {
                    item(key = "__archived__") {
                        ChatArchivedRow(count = list.archived, onClick = if (selecting) null else onOpenArchived)
                    }
                }
                items(section.rows, key = { row -> "chat_${row.sessionId}" }) { row -> chatRowItem(row) }
            }
            // The Archived entry stays first under its header, also with no
            // other chat in that section.
            if (archivedEntry && sections.none { it.key == Chats.ChatSectionKey.Chats }) {
                item(key = "__section_chats__") { ChatSectionHeader(Chats.Copy.SECTION_CHATS) }
                item(key = "__archived__") {
                    ChatArchivedRow(count = list.archived, onClick = if (selecting) null else onOpenArchived)
                }
            }
        }
    }
}

/** A chat: its state mark, the title (it names the issue, EXP-1316: no
 *  identifier slot; EXP-1339: no lock), then a spinner while busy or an
 *  amber dot when it needs you. EXP-1309/1318: on the Chats tab
 *  ([chatsTab], web `ChatsListView`) the row ends on ONE quiet [word]
 *  (`merged` = no text), and its state moves into the lead (web `rowLead`): the
 *  badged mark when it needs you (EXP-1344: beats busy), the working mark
 *  while busy, the emerald `prOpen` glyph while the root has ANY open PR (a
 *  stacked one too) / the violet `prMerged` one when it has only merged ones
 *  (NEW-41, [prLead]), else the mark (dimmed with no word). */
@Composable
internal fun ChatListRow(
    row: SessionTree.ChatsListRow,
    meta: ChatRowMeta?,
    onClick: () -> Unit,
    chatsTab: Boolean = false,
    word: SessionTree.ChatRowWord? = null,
    prLead: ChatRowPrLead? = null,
) {
    val state = SessionTree.chatRowMarkState(row.needsYou, row.busy)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .testTag(SessionTree.ListCopy.TEST_ID_ROW)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (chatsTab) {
            ChatsTabLead(row, meta?.agent, word, prLead)
        } else {
            AgentRunMark(agent = meta?.agent, state = state, size = 14.dp, badgeSize = 6.dp)
        }
        Column(modifier = Modifier.weight(1f)) {
            // EXP-1323: unread news = a bold title ×4.
            Text(
                row.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (row.unread) FontWeight.SemiBold else null,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag(if (row.unread) "chats-row-unread" else "chats-row-title"),
            )
        }
        // EXP-1339: no lock on a list row; `merged` has no text (its lead
        // alone turns pr-merged violet).
        if (chatsTab && word?.text != null) QuietWord(word)
        // EXP-1318 ×4: the ROOT's device, last (after the word),
        // its icon as the Devices screen draws it; unknown device = none.
        meta?.device?.let { device ->
            Icon(
                deviceIcon(device.icon, device.isServer),
                contentDescription = device.name,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(14.dp).testTag("chats-row-device"),
            )
        }
        when {
            chatsTab -> Unit
            row.needsYou -> NeedsYouDot()
            row.busy -> InlineSpinner()
        }
    }
}

/** Web `rowLead`: a Chats-tab row's state, in its lead. */
@Composable
private fun ChatsTabLead(
    row: SessionTree.ChatsListRow,
    agent: String?,
    word: SessionTree.ChatRowWord?,
    prLead: ChatRowPrLead?,
) {
    // EXP-1344 ×4: chatRowMarkState FIRST, at every depth: the amber badge
    // BEATS busy (a parked plan/question keeps the device busy). NEW-41
    // (`chat-list.json`, web `rowLead`): then the PR glyph off the root's
    // PRs themselves, not its word: any open PR (`stack` too) = emerald,
    // else any merged one = violet.
    val mark = SessionTree.chatRowMarkState(row.needsYou, row.busy)
    when {
        mark == SessionTree.SessionState.NeedsYou ->
            AgentRunMark(agent = agent, state = SessionTree.SessionState.NeedsYou, size = 14.dp, badgeSize = 6.dp)
        mark == SessionTree.SessionState.Working -> AgentWorkingMark(agent = agent, size = 14.dp)
        prLead == ChatRowPrLead.Open ->
            Icon(ExpIcons.prOpen, contentDescription = null, tint = DesignTokens.Semantic.Green, modifier = Modifier.size(14.dp))
        prLead == ChatRowPrLead.Merged ->
            Icon(ExpIcons.prMerged, contentDescription = null, tint = DesignTokens.Avatar.Violet, modifier = Modifier.size(14.dp))
        else -> AgentRunMark(
            agent = agent,
            state = if (word == null) SessionTree.SessionState.Done else SessionTree.SessionState.Idle,
            size = 14.dp,
            badgeSize = 6.dp,
        )
    }
}

/** EXP-1309: a row's ONE quiet trailing word (`stack` wears its glyph). */
@Composable
private fun QuietWord(word: SessionTree.ChatRowWord) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.testTag("chats-row-word"),
    ) {
        if (word == SessionTree.ChatRowWord.Stack) {
            Icon(ExpIcons.prStack, contentDescription = null, tint = muted, modifier = Modifier.size(12.dp))
        }
        Text(word.text.orEmpty(), style = MaterialTheme.typography.labelSmall, color = muted, maxLines = 1)
    }
}

/** The amber dot a row carries while it waits on a person. */
@Composable
private fun NeedsYouDot() {
    Box(
        modifier = Modifier
            .size(8.dp)
            .background(DesignTokens.Semantic.Yellow, CircleShape)
            .testTag("chats-row-needs-you"),
    )
}
