package com.exponential.app.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// Thread 58cb32df (fixtures/chats.json): the CHATS pieces Android draws on the
// phone shell — the large headline with its two circles, the agent bubbles,
// the filter chips, the flat WhatsApp row with its 52 lead disc, THE chat
// menu sheet (long-press) + its delete confirm, and the transcript's divider
// pill. One look ×4 (iOS ExpUI, web-mobile, the styleguide entries
// `chat-list`, `chat-menus`, `chat-transcript`).

/** The lead disc's fill (`bg muted`). */
private val DiscFill: Color = DesignTokens.Palette.Muted

/** The inbox circle's unread dot. */
val ChatsInboxDot: Color = DesignTokens.Semantic.Blue

/**
 * The Chats header (Android: the large in-content headline): `copy.title`
 * with the two trailing circles `shell.chatsHeader` — search and the inbox
 * (concept `nav-inbox`, a dot while any notification is unread).
 */
@Composable
fun ChatsHeader(
    onSearch: () -> Unit,
    onInbox: () -> Unit,
    inboxUnread: Boolean,
    modifier: Modifier = Modifier,
    title: String = Chats.Copy.TITLE,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier.fillMaxWidth().padding(start = 20.dp, end = 16.dp, top = 8.dp, bottom = 8.dp),
    ) {
        Text(
            title,
            style = MaterialTheme.typography.headlineLarge.copy(fontWeight = FontWeight.Bold),
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        CircleIconButton(
            ExpIcons.navSearch,
            contentDescription = "Search",
            onClick = onSearch,
            size = 48.dp,
            glyphSize = 20.dp,
            modifier = Modifier.testTag("chats-search"),
        )
        Spacer(Modifier.width(10.dp))
        Box {
            CircleIconButton(
                ExpIcons.navInbox,
                contentDescription = Chats.Copy.INBOX,
                onClick = onInbox,
                size = 48.dp,
                glyphSize = 20.dp,
                modifier = Modifier.testTag(Chats.TestIds.INBOX_BUTTON),
            )
            if (inboxUnread) {
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = (-4).dp, y = 4.dp)
                        .size(10.dp)
                        .background(ChatsInboxDot, CircleShape)
                        .testTag("chats-inbox-dot"),
                )
            }
        }
    }
}

/** The 52 lead DISC of a chat row (`lead`): needs_you = the brand mark with
 *  the amber badge, working = the working mark, open / merged = the root's PR
 *  glyph (emerald / violet), waiting = the bare brand mark. */
@Composable
fun ChatLeadDisc(
    lead: Chats.ChatLead,
    agent: String?,
    modifier: Modifier = Modifier,
    size: Dp = Chats.Shell.LEAD_DISC.dp,
) {
    val glyph = size * 0.5f
    Box(
        modifier = modifier.size(size).clip(CircleShape).background(DiscFill, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        when (lead) {
            Chats.ChatLead.NeedsYou -> AgentRunMark(
                agent = agent,
                state = SessionTree.SessionState.NeedsYou,
                size = glyph,
                badgeSize = size * 0.18f,
            )
            Chats.ChatLead.Working -> AgentWorkingMark(agent = agent, size = glyph)
            Chats.ChatLead.Open -> Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                tint = DesignTokens.Semantic.Green,
                modifier = Modifier.size(glyph),
            )
            Chats.ChatLead.Merged -> Icon(
                ExpIcons.prMerged,
                contentDescription = null,
                tint = DesignTokens.Avatar.Violet,
                modifier = Modifier.size(glyph),
            )
            Chats.ChatLead.Waiting -> AgentRunMark(
                agent = agent,
                state = SessionTree.SessionState.Waiting,
                size = glyph,
                badgeSize = size * 0.18f,
            )
        }
    }
}

/**
 * ONE chat row (`chatList`): the lead disc · the title (semibold while
 * unread) and the time on line 1 (amber while it needs you, foreground while
 * unread, else muted) · the preview (muted, up to two lines; `working…`
 * italic), the row word (with its glyph) and the unread badge on line 2.
 * The hairline runs under the text column only. A tap opens the chat; a
 * long-press opens its menu ([onLongClick], null = no menu).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ChatRow(
    row: Chats.ChatRow,
    agent: String?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    divider: Boolean = true,
) {
    val haptics = LocalHapticFeedback.current
    val unread = row.unreadCount > 0
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val timeColor = when {
        row.needsYou -> NeedsInputAmber
        unread -> MaterialTheme.colorScheme.onSurface
        else -> muted
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .combinedClickable(
                onClick = onClick,
                onLongClick = onLongClick?.let { long ->
                    {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        long()
                    }
                },
            )
            .testTag(Chats.TestIds.CHAT_ROW)
            .padding(start = 20.dp, end = 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ChatLeadDisc(row.lead, agent)
        Spacer(Modifier.width(14.dp))
        Column(modifier = Modifier.weight(1f)) {
            Column(modifier = Modifier.padding(end = 20.dp, top = 14.dp, bottom = 14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        row.title,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = if (unread || row.needsYou) FontWeight.SemiBold else FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).testTag(if (unread) "chat-row-unread" else "chat-row-title"),
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        row.time,
                        style = MaterialTheme.typography.labelMedium,
                        color = timeColor,
                        maxLines = 1,
                        modifier = Modifier.testTag("chat-row-time"),
                    )
                }
                Spacer(Modifier.height(3.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ChatPreviewText(row.preview, Modifier.weight(1f))
                    row.word?.takeIf { it.text != null }?.let { word ->
                        Spacer(Modifier.width(8.dp))
                        ChatRowWord(word)
                    }
                    if (unread) {
                        Spacer(Modifier.width(8.dp))
                        UnreadBadge(row.unreadCount, amber = row.needsYou)
                    }
                }
            }
            if (divider) {
                Box(Modifier.fillMaxWidth().padding(end = 20.dp).height(GlassTokens.Hairline).background(GlassTokens.StrokeCard))
            }
        }
    }
}

@Composable
private fun ChatPreviewText(preview: Chats.ChatPreview, modifier: Modifier = Modifier) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary * 0.85f)
    val style = MaterialTheme.typography.bodyMedium
    // The prefix of a question / plan line reads at the title's colour.
    val prefix = when (preview.kind) {
        "question" -> Chats.Copy.PREVIEW_QUESTION.substringBefore("{text}")
        "plan" -> Chats.Copy.PREVIEW_PLAN.substringBefore("{text}")
        else -> null
    }?.takeIf { preview.text.startsWith(it) }
    val text = if (prefix != null) {
        androidx.compose.ui.text.buildAnnotatedString {
            pushStyle(androidx.compose.ui.text.SpanStyle(color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.Medium))
            append(prefix)
            pop()
            append(preview.text.removePrefix(prefix))
        }
    } else {
        androidx.compose.ui.text.AnnotatedString(preview.text)
    }
    Text(
        text,
        style = style,
        fontStyle = if (preview.kind == "working") FontStyle.Italic else null,
        color = muted,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier.testTag("chat-row-preview"),
    )
}

/** The row word (threadRowWord) with its glyph (`stack` wears the stack). */
@Composable
private fun ChatRowWord(word: SessionTree.ThreadRowWord) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.testTag("chat-row-word"),
    ) {
        val icon = when (word) {
            SessionTree.ThreadRowWord.Stack -> ExpIcons.prStack
            SessionTree.ThreadRowWord.ReadyToMerge -> ExpIcons.prOpen
            SessionTree.ThreadRowWord.Question, SessionTree.ThreadRowWord.NeedsInput -> ExpIcons.codingNeedsInput
            SessionTree.ThreadRowWord.Merged -> null
        }
        if (icon != null) Icon(icon, contentDescription = null, tint = muted, modifier = Modifier.size(14.dp))
        Text(word.text.orEmpty(), style = MaterialTheme.typography.labelMedium, color = muted, maxLines = 1)
    }
}

/** The unread badge: amber with black text while it needs you, else the
 *  foreground with the background's text. */
@Composable
fun UnreadBadge(count: Int, amber: Boolean, modifier: Modifier = Modifier) {
    val fill = if (amber) NeedsInputAmber else MaterialTheme.colorScheme.onSurface
    val ink = if (amber) Color.Black else DesignTokens.Palette.Background
    Box(
        modifier = modifier
            .heightIn(min = 22.dp)
            .widthIn(min = 22.dp)
            .clip(CircleShape)
            .background(fill, CircleShape)
            .padding(horizontal = 6.dp)
            .testTag("chat-row-badge"),
        contentAlignment = Alignment.Center,
    ) {
        Text(count.toString(), style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.SemiBold, color = ink)
    }
}

/**
 * The filter chips (`filters`) above the list: select Pills, labelled by
 * [Chats.chatFilterLabel] (a count after a non-zero one). The row scrolls
 * sideways.
 */
@Composable
fun ChatFilterChips(
    selected: Chats.ChatFilter,
    counts: Chats.ChatCounts,
    onSelect: (Chats.ChatFilter) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Chats.FILTERS.forEach { filter ->
            GlassPill(
                Chats.chatFilterLabel(filter, counts),
                onClick = { onSelect(filter) },
                mode = PillMode.Select,
                selected = filter == selected,
                modifier = Modifier.testTag("${Chats.TestIds.CHAT_FILTER}-${filter.wire}"),
            )
        }
    }
}

/**
 * The Chats header's agent BUBBLES (`agentBubbles`): a 66 circle each (the
 * agent's icon in its colour on bg muted inside a 3px background gap) with
 * the name under it; the ring = amber while the viewer's conversation needs
 * them, foreground while it has unread news, else a hairline. A tap opens the
 * agent's chat, a long-press its agent menu. The row scrolls sideways.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun AgentBubbles(
    bubbles: List<Chats.AgentBubble>,
    onOpen: (SessionTree.AgentRow) -> Unit,
    onLongPress: ((SessionTree.AgentRow) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    if (bubbles.isEmpty()) return
    val haptics = LocalHapticFeedback.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        bubbles.forEach { bubble ->
            val agent = bubble.agent
            val tint = remember(agent.color) { parseColor(agent.color) }
            val icon = ExpIcons.byName(agent.icon)
                ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON)
                ?: ExpIcons.settingsAgents
            val ring = when (bubble.ring) {
                Chats.AgentRing.NeedsYou -> NeedsInputAmber
                Chats.AgentRing.Unread -> MaterialTheme.colorScheme.onSurface
                null -> GlassTokens.StrokeCard
            }
            val ringWidth = if (bubble.ring == null) GlassTokens.Hairline else Chats.Shell.BUBBLE_RING.dp
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier
                    .width(Chats.Shell.BUBBLE.dp + 8.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .combinedClickable(
                        onClick = { onOpen(agent) },
                        onLongClick = onLongPress?.let { long ->
                            {
                                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                long(agent)
                            }
                        },
                    )
                    .testTag(Chats.TestIds.AGENT_BUBBLE),
            ) {
                Box(
                    modifier = Modifier
                        .size(Chats.Shell.BUBBLE.dp)
                        .border(ringWidth, ring, CircleShape)
                        .padding(Chats.Shell.BUBBLE_RING.dp)
                        .clip(CircleShape)
                        .background(DesignTokens.Palette.Background, CircleShape)
                        .padding(Chats.Shell.BUBBLE_RING.dp)
                        .clip(CircleShape)
                        .background(DiscFill, CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    if (agent.busy) {
                        AgentWorkingMark(agent = null, size = 26.dp)
                    } else {
                        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(26.dp))
                    }
                }
                Spacer(Modifier.height(6.dp))
                Text(
                    agent.name,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = if (bubble.ring != null) FontWeight.SemiBold else FontWeight.Normal,
                    color = if (bubble.ring != null) {
                        MaterialTheme.colorScheme.onSurface
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** One entry of THE chat menu sheet. */
data class ChatMenuItem(
    val label: String,
    val icon: ImageVector,
    val onClick: () -> Unit,
    val destructive: Boolean = false,
    val testTag: String? = null,
    /** A hairline above this row (web `agentMenuRows`' separator, iOS
     *  `separates`, the IDE's `menu.separator()`): the agent menu's
     *  conversation pair sits apart from Stop / Agent settings. */
    val dividerBefore: Boolean = false,
)

/** The icon CONCEPT of a chat menu entry (chats.json `menuCases`). */
fun chatMenuIcon(entry: Chats.ThreadMenuEntry): ImageVector = when (entry) {
    Chats.ThreadMenuEntry.Stop -> ExpIcons.codingStop
    Chats.ThreadMenuEntry.StopAndDelete, Chats.ThreadMenuEntry.Delete -> ExpIcons.uiDelete
}

/** The icon CONCEPT of an agent menu entry (`agentMenuCases`). */
fun agentMenuIcon(entry: Chats.AgentMenuEntry): ImageVector = when (entry) {
    Chats.AgentMenuEntry.Compact -> ExpIcons.codingCompact
    Chats.AgentMenuEntry.NewConversation -> ExpIcons.uiClean
    Chats.AgentMenuEntry.Stop -> ExpIcons.codingStop
    Chats.AgentMenuEntry.Settings -> ExpIcons.navSettings
}

/**
 * THE chat menu on a phone (chats.json MENUS ×4: Android's long-press =
 * combinedClickable + this glass menu sheet), titled with the chat's (or
 * agent's) name: one row per entry, the destructive ones in the error colour.
 */
@Composable
fun ChatMenuSheet(
    title: String,
    items: List<ChatMenuItem>,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = title, onDismiss = onDismiss, modifier = Modifier.testTag("chat-menu")) {
        items.forEach { item ->
            if (item.dividerBefore) {
                GroupDivider()
            }
            val tint = if (item.destructive) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            }
            Box(Modifier.then(item.testTag?.let { Modifier.testTag(it) } ?: Modifier)) {
                GlassSheetRow(
                    label = item.label,
                    onClick = item.onClick,
                    labelColor = if (item.destructive) MaterialTheme.colorScheme.error else Color.White.copy(alpha = 0.9f),
                    leading = { Icon(item.icon, contentDescription = null, tint = tint, modifier = Modifier.size(18.dp)) },
                )
            }
        }
        Spacer(Modifier.height(16.dp))
    }
}

/** The chat menu's entries for a chat row: [Chats.threadMenuEntries] with
 *  their copy and icon; Delete / Stop and delete ask first ([onDelete]). */
fun chatMenuItems(
    entries: List<Chats.ThreadMenuEntry>,
    onStop: () -> Unit,
    onDelete: () -> Unit,
): List<ChatMenuItem> = entries.map { entry ->
    ChatMenuItem(
        label = Chats.threadMenuLabel(entry),
        icon = chatMenuIcon(entry),
        onClick = if (entry == Chats.ThreadMenuEntry.Stop) onStop else onDelete,
        destructive = entry != Chats.ThreadMenuEntry.Stop,
        testTag = "chat-menu-${entry.wire}",
    )
}

/** The agent menu's entries ([Chats.agentMenuEntries]) with copy + icon. */
fun agentMenuItems(
    entries: List<Chats.AgentMenuEntry>,
    onPick: (Chats.AgentMenuEntry) -> Unit,
): List<ChatMenuItem> {
    val firstApart = entries.indexOfFirst { it == Chats.AgentMenuEntry.Stop || it == Chats.AgentMenuEntry.Settings }
    return entries.mapIndexed { index, entry ->
        ChatMenuItem(
            label = Chats.agentMenuLabel(entry),
            icon = agentMenuIcon(entry),
            onClick = { onPick(entry) },
            testTag = "agent-menu-${entry.wire}",
            dividerBefore = index > 0 && index == firstApart,
        )
    }
}

/** Delete asks first ×4 (`copy.confirmDelete`; the destructive confirm =
 *  `copy.menu.delete`). */
@Composable
fun ChatDeleteConfirm(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    GlassAlert(
        title = Chats.Copy.CONFIRM_DELETE_TITLE,
        body = Chats.Copy.CONFIRM_DELETE_BODY,
        onDismiss = onDismiss,
        trailing = listOf(
            GlassAlertAction("Cancel", onClick = onDismiss),
            GlassAlertAction(
                Chats.Copy.MENU_DELETE,
                destructive = true,
                testTag = "chat-delete-confirm",
                onClick = onConfirm,
            ),
        ),
        defaultAction = 0,
    )
}

/**
 * A transcript divider (session-thread.json `divider` row): a person's `/new`
 * or `/compact` line painted as the centred glass pill
 * `copy.dividers.<kind> · HH:mm` (icon ui-clean / coding-compact), never a
 * bubble.
 */
@Composable
fun TranscriptDivider(kind: String, time: String?, modifier: Modifier = Modifier) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Row(
            modifier = Modifier
                .clip(CircleShape)
                .background(GlassTokens.CardFill, CircleShape)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape)
                .padding(horizontal = 14.dp, vertical = 6.dp)
                .testTag(Chats.TestIds.DIVIDER),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                if (kind == Chats.DIVIDER_KIND_COMPACT) ExpIcons.codingCompact else ExpIcons.uiClean,
                contentDescription = null,
                tint = muted,
                modifier = Modifier.size(14.dp),
            )
            Text(
                listOfNotNull(Chats.Copy.divider(kind), time?.takeIf { it.isNotEmpty() }).joinToString(" · "),
                style = MaterialTheme.typography.labelMedium,
                color = muted,
                maxLines = 1,
            )
        }
    }
}

/** A day pill over the transcript (`Today`, `Yesterday`, a date). */
@Composable
fun TranscriptDayPill(label: String, modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = Modifier
                .clip(CircleShape)
                .background(GlassTokens.CardFill, CircleShape)
                .padding(horizontal = 12.dp, vertical = 5.dp),
        )
    }
}
