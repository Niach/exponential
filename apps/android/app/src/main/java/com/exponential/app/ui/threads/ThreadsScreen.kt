package com.exponential.app.ui.threads

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
import androidx.compose.foundation.lazy.itemsIndexed
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
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.AgentBubbles
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.AgentWorkingMark
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.ChatDeleteConfirm
import com.exponential.app.ui.components.ChatFilterChips
import com.exponential.app.ui.components.ChatMenuSheet
import com.exponential.app.ui.components.ChatRow
import com.exponential.app.ui.components.LocalPageHeaderActions
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.agentMenuItems
import com.exponential.app.ui.components.chatMenuItems
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * Thread 58cb32df: the CHATS tab (chats.json LIST, phones). The large
 * in-content headline `copy.title` with its search + inbox circles, the
 * agent BUBBLES row, the filter chips, then ONE flat list of the active
 * team's chats ([Chats.chatList]: the roots of partitionThreads by
 * activity, no bands, no nesting). A row opens its chat; a long-press opens
 * THE chat menu sheet (Stop · Stop and delete live, Delete ended; Delete asks
 * first). A bubble opens the agent's chat; its long-press the agent menu.
 */
@Composable
fun ThreadsScreen(
    onOpenThread: (rootSessionId: String) -> Unit,
    /** EXP-1323: kept for the route's signature (the phone list has no child rows). */
    onOpenSession: (sessionId: String) -> Unit = onOpenThread,
    /** NEW-7: an agent with no conversation of mine yet = its empty DM. */
    onOpenAgent: (agentId: String) -> Unit = {},
    /** chats.json: the header's inbox circle pushes the inbox-only page. */
    onOpenInbox: () -> Unit = {},
    /** The agent menu's Agent settings (owners). */
    onOpenAgentSettings: (agentId: String) -> Unit = {},
    /** The inbox circle's dot: any notification unread. */
    inboxUnread: Boolean = false,
    viewModel: ThreadsViewModel = hiltViewModel(),
) {
    val list by viewModel.chatList.collectAsStateWithLifecycle()
    val filter by viewModel.filter.collectAsStateWithLifecycle()
    val bubbles by viewModel.bubbles.collectAsStateWithLifecycle()
    val conversations by viewModel.conversations.collectAsStateWithLifecycle()
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

    var menuFor by remember { mutableStateOf<Chats.ChatRow?>(null) }
    var agentMenuFor by remember { mutableStateOf<SessionTree.AgentRow?>(null) }
    var deleteFor by remember { mutableStateOf<String?>(null) }

    Scaffold(containerColor = Color.Transparent) { padding ->
        ChatsContent(
            list = list,
            filter = filter,
            bubbles = bubbles,
            agentOf = { facts[it]?.agent },
            onSelectFilter = viewModel::selectFilter,
            onOpenChat = onOpenThread,
            onLongPressChat = { row ->
                // A viewer who cannot steer the chat gets no menu.
                val root = facts[row.sessionId]
                if (root != null && Chats.threadMenuEntries(root.status, root.canSteer, root.mine || isOwner).isNotEmpty()) menuFor = row
            },
            onOpenAgent = { agent -> conversations[agent.agentId]?.let(onOpenThread) ?: onOpenAgent(agent.agentId) },
            onLongPressAgent = { agentMenuFor = it },
            onOpenInbox = onOpenInbox,
            inboxUnread = inboxUnread,
            modifier = Modifier.padding(padding),
        )
    }

    menuFor?.let { row ->
        val root = facts[row.sessionId]
        val entries = root?.let { Chats.threadMenuEntries(it.status, it.canSteer, it.mine || isOwner) }.orEmpty()
        ChatMenuSheet(
            title = row.title,
            items = chatMenuItems(
                entries,
                onStop = {
                    menuFor = null
                    viewModel.stopChat(row.sessionId)
                },
                onDelete = {
                    menuFor = null
                    deleteFor = row.sessionId
                },
            ),
            onDismiss = { menuFor = null },
        )
    }
    agentMenuFor?.let { agent ->
        val conversation = conversations[agent.agentId]
        ChatMenuSheet(
            title = agent.name,
            items = agentMenuItems(Chats.agentMenuEntries(busy = agent.busy, isOwner = isOwner)) { entry ->
                agentMenuFor = null
                when (entry) {
                    Chats.AgentMenuEntry.Compact, Chats.AgentMenuEntry.NewConversation ->
                        Chats.agentMenuSend(entry)?.let { viewModel.sendToAgent(agent.agentId, it) }
                    Chats.AgentMenuEntry.Stop -> conversation?.let(viewModel::stopTurn)
                    Chats.AgentMenuEntry.Settings -> onOpenAgentSettings(agent.agentId)
                }
            },
            onDismiss = { agentMenuFor = null },
        )
    }
    deleteFor?.let { rootId ->
        ChatDeleteConfirm(
            onConfirm = {
                deleteFor = null
                viewModel.deleteChat(rootId)
            },
            onDismiss = { deleteFor = null },
        )
    }
}

/** The Chats tab's stateless body (the styleguide's `chat-list` draws it). */
@Composable
fun ChatsContent(
    list: Chats.ChatList,
    filter: Chats.ChatFilter,
    bubbles: List<Chats.AgentBubble>,
    agentOf: (rootId: String) -> String?,
    onSelectFilter: (Chats.ChatFilter) -> Unit,
    onOpenChat: (rootSessionId: String) -> Unit,
    onLongPressChat: (Chats.ChatRow) -> Unit,
    onOpenAgent: (SessionTree.AgentRow) -> Unit,
    onLongPressAgent: (SessionTree.AgentRow) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Boolean,
    modifier: Modifier = Modifier,
) {
    // #EXP-1384: the header's search pushes the phone search; the inbox
    // circle is this list's own (the specimen drives it).
    val shell = LocalPageHeaderActions.current
    val headerActions = PageHeaderActions(
        onSearch = { shell?.onSearch?.invoke() },
        onInbox = onOpenInbox,
        inboxUnread = inboxUnread,
    )
    val rows = list.rows
    LazyColumn(
        modifier = modifier.fillMaxSize().testTag(SessionTree.ListCopy.TEST_ID_LIST),
        contentPadding = PaddingValues(top = 4.dp, bottom = BottomBarInset),
    ) {
        item(key = "__header__") {
            PageHeader(title = Chats.Copy.HEADERS.getValue("chats"), actions = headerActions)
        }
        if (bubbles.isNotEmpty()) {
            item(key = "__bubbles__") {
                AgentBubbles(bubbles = bubbles, onOpen = onOpenAgent, onLongPress = onLongPressAgent)
            }
        }
        item(key = "__filters__") {
            ChatFilterChips(selected = filter, counts = list.counts, onSelect = onSelectFilter)
        }
        if (rows.isEmpty()) {
            item(key = "__empty__") {
                Box(Modifier.fillMaxWidth().height(320.dp)) { EmptyState(EmptyStateCopy.threads) }
            }
        }
        itemsIndexed(rows, key = { _, row -> "chat_${row.sessionId}" }) { index, row ->
            ChatRow(
                row = row,
                agent = agentOf(row.sessionId),
                onClick = { onOpenChat(row.sessionId) },
                onLongClick = { onLongPressChat(row) },
                divider = index < rows.lastIndex,
            )
        }
    }
}

/** A thread: its state mark, the title (it names the issue, EXP-1316: no
 *  identifier slot; EXP-1339: no lock), then a spinner while busy or an
 *  amber dot when it needs you. EXP-1309/1318: on the Threads tab
 *  ([threadsTab], web `ThreadsListView`) the row ends on ONE quiet [word]
 *  (`merged` = no text), and its state moves into the lead (web `rowLead`): the
 *  badged mark when it needs you (EXP-1344: beats busy), the working mark
 *  while busy, the emerald `prOpen` glyph while the root has ANY open PR (a
 *  stacked one too) / the violet `prMerged` one when it has only merged ones
 *  (NEW-41, [prLead]), else the mark (dimmed with no word). */
@Composable
internal fun ThreadListRow(
    row: SessionTree.ThreadsListRow,
    meta: ThreadRowMeta?,
    onClick: () -> Unit,
    threadsTab: Boolean = false,
    word: SessionTree.ThreadRowWord? = null,
    prLead: ThreadRowPrLead? = null,
) {
    val state = SessionTree.threadRowMarkState(row.needsYou, row.busy)
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
        if (threadsTab) {
            ThreadsTabLead(row, meta?.agent, word, prLead)
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
                modifier = Modifier.testTag(if (row.unread) "threads-row-unread" else "threads-row-title"),
            )
        }
        // EXP-1339: no lock on a list row; `merged` has no text (its lead
        // alone turns pr-merged violet).
        if (threadsTab && word?.text != null) QuietWord(word)
        // EXP-1318 ×4: the ROOT's device, last (after the word),
        // its icon as the Devices screen draws it; unknown device = none.
        meta?.device?.let { device ->
            Icon(
                deviceIcon(device.icon, device.isServer),
                contentDescription = device.name,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(14.dp).testTag("threads-row-device"),
            )
        }
        when {
            threadsTab -> Unit
            row.needsYou -> NeedsYouDot()
            row.busy -> InlineSpinner()
        }
    }
}

/** Web `rowLead`: a Threads-tab row's state, in its lead. */
@Composable
private fun ThreadsTabLead(
    row: SessionTree.ThreadsListRow,
    agent: String?,
    word: SessionTree.ThreadRowWord?,
    prLead: ThreadRowPrLead?,
) {
    // EXP-1344 ×4: threadRowMarkState FIRST, at every depth: the amber badge
    // BEATS busy (a parked plan/question keeps the device busy). NEW-41
    // (`threads-list.json`, web `rowLead`): then the PR glyph off the root's
    // PRs themselves, not its word: any open PR (`stack` too) = emerald,
    // else any merged one = violet.
    val mark = SessionTree.threadRowMarkState(row.needsYou, row.busy)
    when {
        mark == SessionTree.SessionState.NeedsYou ->
            AgentRunMark(agent = agent, state = SessionTree.SessionState.NeedsYou, size = 14.dp, badgeSize = 6.dp)
        mark == SessionTree.SessionState.Working -> AgentWorkingMark(agent = agent, size = 14.dp)
        prLead == ThreadRowPrLead.Open ->
            Icon(ExpIcons.prOpen, contentDescription = null, tint = DesignTokens.Semantic.Green, modifier = Modifier.size(14.dp))
        prLead == ThreadRowPrLead.Merged ->
            Icon(ExpIcons.prMerged, contentDescription = null, tint = DesignTokens.Avatar.Violet, modifier = Modifier.size(14.dp))
        else -> AgentRunMark(
            agent = agent,
            state = if (word == null) SessionTree.SessionState.Ended else SessionTree.SessionState.Waiting,
            size = 14.dp,
            badgeSize = 6.dp,
        )
    }
}

/** EXP-1309: a row's ONE quiet trailing word (`stack` wears its glyph). */
@Composable
private fun QuietWord(word: SessionTree.ThreadRowWord) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.testTag("threads-row-word"),
    ) {
        if (word == SessionTree.ThreadRowWord.Stack) {
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
            .testTag("threads-row-needs-you"),
    )
}
