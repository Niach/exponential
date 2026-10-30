package com.exponential.app.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
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
import com.exponential.app.domain.SealedPlaceholder
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.stories.StoryAvatar
import com.exponential.app.ui.stories.StoryRingButton
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// Thread 58cb32df (fixtures/chats.json): the CHATS pieces Android draws on the
// phone shell (the header is the shared [PageHeader]) — the agent bubbles,
// the filter chips, the flat WhatsApp row with its 52 lead disc, THE chat
// menu sheet (long-press) + its delete confirm, and the transcript's divider
// pill. One look ×4 (iOS ExpUI, web-mobile, the styleguide entries
// `chat-list`, `chat-menus`, `chat-transcript`).

/** The lead disc's fill (`bg muted`). */
private val DiscFill: Color = DesignTokens.Palette.Muted

/** The inbox circle's unread dot. */
val ChatsInboxDot: Color = DesignTokens.Semantic.Blue

/** The 52 lead DISC of a chat row (`lead`): S-3 needs_you = the chat glyph
 *  (`shell.chatLead`) with the amber badge, working = the working mark, open
 *  / merged = the root's PR glyph (emerald / violet), waiting = the plain
 *  chat glyph. */
@Composable
fun ChatLeadDisc(
    lead: Chats.ChatLead,
    agent: String?,
    modifier: Modifier = Modifier,
    size: Dp = Chats.Shell.LEAD_DISC.dp,
    selected: Boolean = false,
) {
    val glyph = size * 0.5f
    Box(
        modifier = modifier.size(size).clip(CircleShape).background(DiscFill, CircleShape)
            // #EXP-1388: a selected row's disc steps one grey notch up.
            .then(if (selected) Modifier.background(GlassTokens.RowFillActive, CircleShape) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        when (lead) {
            // S-3 (chats.json `shell.chatLead`): the plain chat glyph, never
            // the brand mark; needs you = that glyph + the amber badge.
            Chats.ChatLead.NeedsYou -> Box {
                Icon(
                    ExpIcons.actionChat,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.size(glyph),
                )
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .size(size * 0.18f)
                        .clip(CircleShape)
                        .background(NeedsInputAmber, CircleShape),
                )
            }
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
            Chats.ChatLead.Waiting -> Icon(
                ExpIcons.actionChat,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.size(glyph),
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
 * long-press enters the selection ([onLongClick], null = none; #EXP-1388;
 * the caller fires the haptic when it ENTERS, like the issue list).
 * While selecting ([selected] non-null) NOTHING shifts (touch, WhatsApp's
 * selection): an unselected row looks as it does outside the selection; a
 * selected row = the full-width flat `fillActive` tint + [ChatSelectBadge]
 * on its (one notch stronger) lead disc.
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
    selected: Boolean? = null,
) {
    val unread = row.unreadCount > 0
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val timeColor = when {
        row.needsYou -> NeedsInputAmber
        unread -> MaterialTheme.colorScheme.onSurface
        else -> muted
    }
    val isSelected = selected == true
    Row(
        modifier = modifier
            .fillMaxWidth()
            // Edge to edge, no radius, no hairline.
            .then(if (isSelected) Modifier.background(GlassTokens.RowFillActive) else Modifier)
            // S-3: a done chat (every root PR merged, nothing live) stays,
            // the WHOLE row at `shell.dimOpacity`.
            .then(if (row.dim) Modifier.alpha(Chats.Shell.DIM_OPACITY) else Modifier)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .testTag(Chats.TestIds.CHAT_ROW)
            .padding(start = ChatRowEdge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            ChatLeadDisc(row.lead, agent, selected = isSelected)
            if (isSelected) ChatSelectBadge(Modifier.align(Alignment.BottomEnd))
        }
        Spacer(Modifier.width(14.dp))
        Column(modifier = Modifier.weight(1f)) {
            Column(modifier = Modifier.padding(end = ChatRowEdge, top = 14.dp, bottom = 14.dp)) {
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
                // A selected row is one flat tint; the hairline keeps its
                // height so selecting never reflows the list.
                Box(
                    Modifier.fillMaxWidth().padding(end = ChatRowEdge).height(GlassTokens.Hairline)
                        .then(if (isSelected) Modifier else Modifier.background(GlassTokens.StrokeCard)),
                )
            }
        }
    }
}

/** S-3 (chats.json `copy.sections`): the Agents / Chats header over the
 *  list's two sections (only while an agent row exists). */
@Composable
fun ChatSectionHeader(label: String, modifier: Modifier = Modifier) {
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = modifier
            .fillMaxWidth()
            .padding(start = ChatRowEdge, end = ChatRowEdge, top = 12.dp, bottom = 4.dp)
            .testTag("chat-section-header"),
    )
}

/**
 * S-3 (chats.json `agentChatRows`): ONE agent row pinned on top of the list
 * = THE chat row's layout with the agent's disc as its lead (the viewer's
 * Assistant = the brand mark), its name, the conversation's preview, time
 * and unread badge. A tap opens the agent's chat.
 */
@Composable
fun AgentChatRowItem(
    row: Chats.AgentChatRow,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    divider: Boolean = true,
) {
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
            .clickable(onClick = onClick)
            .testTag("chat-agent-row")
            .padding(start = ChatRowEdge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // chats.json AGENT ROWS: the avatar ALWAYS (busy reads in the
        // preview, never as the working mark); needs you = the chat lead's
        // amber badge on it (web `ChatLeadDisc` `run-mark-badge`).
        Box(modifier = Modifier.size(Chats.Shell.LEAD_DISC.dp)) {
            AgentBubbleDisc(
                icon = row.icon,
                color = row.color,
                assistant = row.assistant,
                size = Chats.Shell.LEAD_DISC.dp,
                ring = Color.Transparent,
                ringWidth = 0.dp,
                ringGap = 0.dp,
                glyph = (Chats.Shell.LEAD_DISC * 0.5f).dp,
            )
            if (row.needsYou) {
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = (-8).dp, y = 8.dp)
                        .size(14.dp)
                        .clip(CircleShape)
                        .background(DiscFill, CircleShape)
                        .padding(2.dp)
                        .clip(CircleShape)
                        .background(NeedsInputAmber, CircleShape)
                        .testTag("chat-agent-row-needs-you"),
                )
            }
        }
        Spacer(Modifier.width(14.dp))
        Column(modifier = Modifier.weight(1f)) {
            Column(modifier = Modifier.padding(end = ChatRowEdge, top = 14.dp, bottom = 14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        row.name,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = if (unread || row.needsYou) FontWeight.SemiBold else FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    if (row.time.isNotEmpty()) {
                        Spacer(Modifier.width(8.dp))
                        Text(row.time, style = MaterialTheme.typography.labelMedium, color = timeColor, maxLines = 1)
                    }
                }
                Spacer(Modifier.height(3.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ChatPreviewText(row.preview, Modifier.weight(1f))
                    if (unread) {
                        Spacer(Modifier.width(8.dp))
                        UnreadBadge(row.unreadCount, amber = row.needsYou)
                    }
                }
            }
            if (divider) {
                Box(Modifier.fillMaxWidth().padding(end = ChatRowEdge).height(GlassTokens.Hairline).background(GlassTokens.StrokeCard))
            }
        }
    }
}

/** A chat row's text edge from the screen edge (both sides). */
private val ChatRowEdge: Dp = 20.dp

/** The selected row's ground: the screen under the `fillActive` tint. */
private val SelectedRowGround: Color = GlassTokens.RowFillActive.compositeOver(DesignTokens.Palette.Background)

/**
 * #EXP-1388 THE SELECT BADGE (styleguide `chat-selection`): THE checkbox
 * square (owner 2026-10-27: every check is the square Checkbox) =
 * `shell.selectBadge` primary fill at `shell.selectBadgeRadius` holding
 * ui-check (`shell.selectBadgeGlyph`, primary-foreground, its normal stroke)
 * inside a `shell.selectBadgeRing` ring in the selected row's ground (outer
 * corner = radius + ring), so it reads cut out of the disc. Its outer box
 * sits flush with the disc's bottom-right.
 */
@Composable
fun ChatSelectBadge(modifier: Modifier = Modifier) {
    val ring = Chats.Shell.SELECT_BADGE_RING.dp
    val radius = Chats.Shell.SELECT_BADGE_RADIUS.dp
    Box(
        modifier = modifier
            .size(Chats.Shell.SELECT_BADGE.dp + ring * 2)
            .background(SelectedRowGround, RoundedCornerShape(radius + ring))
            .padding(ring)
            .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(radius)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.uiCheck,
            contentDescription = "Selected",
            tint = MaterialTheme.colorScheme.onPrimary,
            modifier = Modifier.size(Chats.Shell.SELECT_BADGE_GLYPH.dp),
        )
    }
}

/**
 * #EXP-1388 THE ARCHIVED ENTRY: the list's first row while chats are
 * archived (filter All) = a chat row whose lead cell holds the PLAIN
 * ui-archive glyph (`shell.archivedGlyph`, muted, no disc, centred in the
 * `leadDisc` column), the title `copy.archived.entry` and the count in the
 * time slot (muted), no preview line; as tall as its text line. A tap opens
 * the Archived page ([onClick] null = inert, while selecting); it never moves.
 */
@Composable
fun ChatArchivedRow(
    count: Int,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    divider: Boolean = true,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .testTag(Chats.TestIds.CHAT_ARCHIVED)
            .padding(start = ChatRowEdge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(modifier = Modifier.width(Chats.Shell.LEAD_DISC.dp), contentAlignment = Alignment.Center) {
            Icon(
                ExpIcons.uiArchive,
                contentDescription = null,
                tint = muted,
                modifier = Modifier.size(Chats.Shell.ARCHIVED_GLYPH.dp),
            )
        }
        Spacer(Modifier.width(14.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(
                modifier = Modifier.padding(end = ChatRowEdge, top = 14.dp, bottom = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    Chats.Copy.ARCHIVED_ENTRY,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    count.toString(),
                    style = MaterialTheme.typography.labelMedium,
                    color = muted,
                    maxLines = 1,
                    modifier = Modifier.testTag("chat-archived-count"),
                )
            }
            if (divider) {
                Box(Modifier.fillMaxWidth().padding(end = ChatRowEdge).height(GlassTokens.Hairline).background(GlassTokens.StrokeCard))
            }
        }
    }
}

/** The icon CONCEPT of a selection bar action (chats.json `selectionCases`). */
fun chatSelectionIcon(action: Chats.ChatSelectionAction): ImageVector = when (action) {
    Chats.ChatSelectionAction.MarkRead -> ExpIcons.uiMarkRead
    Chats.ChatSelectionAction.MarkUnread -> ExpIcons.uiMarkUnread
    Chats.ChatSelectionAction.Archive -> ExpIcons.uiArchive
    Chats.ChatSelectionAction.Unarchive -> ExpIcons.uiUnarchive
    Chats.ChatSelectionAction.Delete -> ExpIcons.uiDelete
}

/**
 * #EXP-1388 THE Chats selection bar = the issue list's bulk bar
 * ([SelectionBarShell]: same card, close, count) with
 * [Chats.chatSelectionActions] as its icon buttons (Delete in the error
 * colour, last).
 */
@Composable
fun ChatSelectionBar(
    count: Int,
    actions: List<Chats.ChatSelectionEntry>,
    onClear: () -> Unit,
    onAction: (Chats.ChatSelectionEntry) -> Unit,
    modifier: Modifier = Modifier,
) {
    SelectionBarShell(
        count = Chats.Copy.selectionCount(count),
        onClear = onClear,
        modifier = modifier,
        testTag = Chats.TestIds.SELECTION_BAR,
    ) {
        actions.forEach { entry ->
            SelectionBarIconButton(
                icon = chatSelectionIcon(entry.action),
                contentDescription = Chats.chatSelectionLabel(entry.action),
                onClick = { onAction(entry) },
                modifier = Modifier.testTag("chat-selection-${entry.action.wire}"),
                tint = if (entry.action == Chats.ChatSelectionAction.Delete) {
                    MaterialTheme.colorScheme.error
                } else {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                },
            )
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
        // EXP-1459: a locked preview reads italic muted like `working`.
        fontStyle = if (preview.kind == "working" || preview.kind == Chats.PREVIEW_KIND_LOCKED) FontStyle.Italic else null,
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
 * THE STORY ROW of the Chats header (stories.json STORY ROW, #EXP-1461):
 * ONLY rings ([Stories.storyRow]). A bubble whose agent has a ring (the last
 * 24 h) wears it, in listAgents order (the agent's icon in its colour, its
 * name under it, medium while unseen; a tap opens the viewer at the ring, a
 * long-press its agent menu); a bubble without one is not drawn; the rings
 * no bubble wears follow in ring order. Nothing = no row (no height). Each
 * item is exactly `row.itemWidth` wide, `row.gap` apart, from the chips'
 * leading edge; the row scrolls sideways.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun AgentBubbles(
    bubbles: List<Chats.AgentBubble>,
    onLongPress: ((SessionTree.AgentRow) -> Unit)?,
    modifier: Modifier = Modifier,
    /** The story rings ([Stories.rings], ring order). */
    rings: List<Stories.StoryRing> = emptyList(),
    /** A trailing ring's avatar (a root ring's brand mark). */
    ringAvatar: (Stories.StoryRing) -> StoryAvatar = { StoryAvatar(null, null) },
    onOpenStory: (key: String) -> Unit = {},
) {
    val row = remember(bubbles, rings) { Stories.storyRow(bubbles.map { it.agent.agentId }, rings) }
    if (row.isEmpty) return
    val worn = remember(bubbles, row) { bubbles.filter { it.agent.agentId in row.byAgent } }
    val haptics = LocalHapticFeedback.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(start = 20.dp, end = 20.dp, top = 6.dp, bottom = Stories.Row.PAD_BOTTOM.dp),
        horizontalArrangement = Arrangement.spacedBy(Stories.Row.GAP.dp),
    ) {
        worn.forEach { bubble ->
            val agent = bubble.agent
            val story = row.byAgent.getValue(agent.agentId)
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier
                    .width(Stories.Row.ITEM_WIDTH.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .combinedClickable(
                        onClick = { onOpenStory(story.key) },
                        onLongClick = onLongPress?.let { long ->
                            {
                                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                long(agent)
                            }
                        },
                    )
                    .testTag(Chats.TestIds.AGENT_BUBBLE),
            ) {
                // THE story ring at the bubble size; its inner muted disc
                // carries the bubble's avatar / working mark.
                StoryRing(stories = story.stories, size = Chats.Shell.BUBBLE.dp) {
                    AgentBubbleDisc(
                        icon = agent.icon,
                        color = agent.color,
                        assistant = agent.assistant,
                        size = Chats.Shell.BUBBLE.dp - (Stories.Ring.WIDTH.dp + Chats.Shell.BUBBLE_RING.dp) * 2,
                        ring = Color.Transparent,
                        ringWidth = 0.dp,
                        ringGap = 0.dp,
                        glyph = 26.dp,
                        busy = agent.busy,
                    )
                }
                Spacer(Modifier.height(Stories.Row.LABEL_GAP.dp))
                Text(
                    agent.name,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = if (story.unseen) FontWeight.SemiBold else FontWeight.Normal,
                    color = if (story.unseen) {
                        MaterialTheme.colorScheme.onSurface
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        row.rest.forEach { ring -> StoryRingButton(ring = ring, avatar = ringAvatar(ring), onOpen = onOpenStory) }
    }
}

/**
 * S-3: THE agent bubble disc (chats.json `bubble`, composer-home.json
 * `agentRow`): the agent's icon in its colour on bg muted inside a
 * [ringGap] background gap, wearing [ring]. The viewer's Assistant paints
 * the Exponential brand mark in the foreground colour instead.
 */
@Composable
fun AgentBubbleDisc(
    icon: String?,
    color: String?,
    assistant: Boolean,
    size: Dp,
    ring: Color,
    ringWidth: Dp,
    ringGap: Dp,
    glyph: Dp,
    modifier: Modifier = Modifier,
    busy: Boolean = false,
) {
    val tint = remember(color) { parseColor(color ?: SessionTree.ListCopy.AGENT_DEFAULT_COLOR) }
    val vector = icon?.let { ExpIcons.byName(it) }
        ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        ?: ExpIcons.settingsAgents
    Box(
        modifier = modifier
            .size(size)
            .border(ringWidth, ring, CircleShape)
            .padding(ringGap)
            .clip(CircleShape)
            .background(DesignTokens.Palette.Background, CircleShape)
            .padding(ringGap)
            .clip(CircleShape)
            .background(DiscFill, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        when {
            busy -> AgentWorkingMark(agent = null, size = glyph)
            assistant -> ExponentialMark(size = glyph, tint = MaterialTheme.colorScheme.onSurface)
            else -> Icon(vector, contentDescription = null, tint = tint, modifier = Modifier.size(glyph))
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
     *  conversation pair sits apart from Agent settings. */
    val dividerBefore: Boolean = false,
)

/** The icon CONCEPT of a chat menu entry (chats.json `menuCases`). */
fun chatMenuIcon(entry: Chats.ThreadMenuEntry): ImageVector = when (entry) {
    Chats.ThreadMenuEntry.Delete -> ExpIcons.uiDelete
}

/** The icon CONCEPT of an agent menu entry (`agentMenuCases`). */
fun agentMenuIcon(entry: Chats.AgentMenuEntry): ImageVector = when (entry) {
    Chats.AgentMenuEntry.Compact -> ExpIcons.codingCompact
    Chats.AgentMenuEntry.NewConversation -> ExpIcons.uiClean
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
        ChatMenuRows(items)
        Spacer(Modifier.height(16.dp))
    }
}

/** THE chat menu sheet's rows (the sheet's body; the styleguide's static
 *  faces draw the same rows). */
@Composable
fun ChatMenuRows(items: List<ChatMenuItem>) {
    Column {
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
    }
}

/**
 * THE chat menu sheet's face in place (the styleguide's `chat-menus`, iOS's
 * static sheet faces line for line): the sheet's fill and top corners, its
 * title, its rows — what [ChatMenuSheet] shows, without the modal.
 */
@Composable
fun ChatMenuSheetFace(title: String, items: List<ChatMenuItem>, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(GlassSheetDefaults.CornerRadius)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassSheetDefaults.ContainerColor, shape)
            .padding(top = 14.dp, bottom = 8.dp),
    ) {
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            color = GlassSheetDefaults.TitleColor,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding).padding(bottom = 6.dp),
        )
        ChatMenuRows(items)
    }
}

/** The chat menu's entries for a chat row: [Chats.threadMenuEntries] with
 *  their copy and icon; Delete asks first ([onDelete]). S-3: no Stop. */
fun chatMenuItems(
    entries: List<Chats.ThreadMenuEntry>,
    onDelete: () -> Unit,
): List<ChatMenuItem> = entries.map { entry ->
    ChatMenuItem(
        label = Chats.threadMenuLabel(entry),
        icon = chatMenuIcon(entry),
        onClick = onDelete,
        destructive = true,
        testTag = "chat-menu-${entry.wire}",
    )
}

/** The agent menu's entries ([Chats.agentMenuEntries]) with copy + icon. */
fun agentMenuItems(
    entries: List<Chats.AgentMenuEntry>,
    onPick: (Chats.AgentMenuEntry) -> Unit,
): List<ChatMenuItem> {
    val firstApart = entries.indexOfFirst { it == Chats.AgentMenuEntry.Settings }
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
fun ChatDeleteConfirm(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    /** #EXP-1388: [Chats.chatDeleteTitle] for a selection of several. */
    title: String = Chats.Copy.CONFIRM_DELETE_TITLE,
) {
    GlassAlert(
        title = title,
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
 * bubble. EXP-1459: kind `locked-noKey` | `locked-unreadable` = the same pill
 * with the lock (sealed-content.json `placeholder`), no time.
 */
@Composable
fun TranscriptDivider(kind: String, time: String?, modifier: Modifier = Modifier) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val locked = kind.removePrefix(LOCKED_DIVIDER_PREFIX).takeIf { kind.startsWith(LOCKED_DIVIDER_PREFIX) }
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Row(
            modifier = Modifier
                .clip(CircleShape)
                .background(GlassTokens.CardFill, CircleShape)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape)
                .padding(horizontal = 14.dp, vertical = 6.dp)
                .testTag(if (locked != null) SEALED_LOCKED_TEST_ID else Chats.TestIds.DIVIDER),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                when {
                    locked != null -> ExpIcons.uiPrivate
                    kind == Chats.DIVIDER_KIND_COMPACT -> ExpIcons.codingCompact
                    else -> ExpIcons.uiClean
                },
                contentDescription = null,
                tint = muted,
                modifier = Modifier.size(14.dp),
            )
            Text(
                if (locked != null) SealedPlaceholder.copy(locked)
                else listOfNotNull(Chats.Copy.divider(kind), time?.takeIf { it.isNotEmpty() }).joinToString(" · "),
                style = MaterialTheme.typography.labelMedium,
                color = muted,
                maxLines = 1,
            )
        }
    }
}

/** EXP-1459: the [TranscriptDivider] kind of a locked row (`locked-<reason>`). */
const val LOCKED_DIVIDER_PREFIX = "locked-"

/** EXP-1459: the locked pill's test tag (component tests, captures). */
const val SEALED_LOCKED_TEST_ID = "sealed-locked"

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
