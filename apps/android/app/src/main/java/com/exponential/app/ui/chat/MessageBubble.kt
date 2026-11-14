package com.exponential.app.ui.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.components.AgentAvatar
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.LocalAgentMarks
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// #EXP-1736 (transcript-grouping.json): THE message bubble ×4, every chat a
// group chat (web packages/ui `MessageBubble` + `SenderAvatar` +
// `TranscriptRow`, desktop `message_bubble`, iOS ExpUI `MessageBubble`).

private val BubbleMuted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

/**
 * THE message bubble: the glass card bubble (16dp radius, its `HH:mm`
 * bottom-right inside), at most `layout.maxWidth.phoneIn` of its column;
 * #FEED-239: an [own] bubble `maxWidth.own` (the whole column left of the
 * viewer's avatar). [own]
 * = the viewer's (right, the active glass fill under the strong hairline, no
 * name); else anyone else's (left, THE card surface) with the sender's name
 * line [author] (12sp medium, muted; the group's first bubble only) and, for
 * a cross-session sender, [onClick] on the whole bubble (that session).
 * #FEED-237 (message-delivery.json): the viewer's own bubble carries its
 * delivery mark [status] right after the `HH:mm` (the clock, one check, two
 * checks, the red mark; every other sender's has none); [onLongClick] opens
 * its menu, a failed mark is itself a press target ([onStatusClick]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun MessageBubble(
    own: Boolean,
    time: String?,
    modifier: Modifier = Modifier,
    author: String? = null,
    onClick: (() -> Unit)? = null,
    queued: Boolean = false,
    onUnqueue: (() -> Unit)? = null,
    testTag: String = if (own) "chat-message-bubble" else "chat-agent-message",
    status: MessageDelivery.State? = null,
    onLongClick: (() -> Unit)? = null,
    onStatusClick: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val max = maxWidth * if (own) TranscriptGrouping.Layout.MAX_WIDTH_OWN else TranscriptGrouping.Layout.MAX_WIDTH_PHONE_IN
        val shape = RoundedCornerShape(TranscriptGrouping.Layout.RADIUS.dp)
        Column(
            modifier = Modifier
                .align(if (own) Alignment.CenterEnd else Alignment.CenterStart)
                .widthIn(max = max)
                .clip(shape)
                .background(if (own) GlassTokens.RowFillActive else GlassTokens.OpaqueCardFill, shape)
                .border(GlassTokens.Hairline, if (own) GlassTokens.StrokeStrong else GlassTokens.StrokeCard, shape)
                .then(
                    when {
                        onLongClick != null -> Modifier.combinedClickable(onClick = onClick ?: {}, onLongClick = onLongClick)
                        onClick != null -> Modifier.clickable(role = Role.Button, onClick = onClick)
                        else -> Modifier
                    },
                )
                .padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 10.dp)
                .testTag(testTag),
        ) {
            val name = author?.takeIf { !own && it.isNotEmpty() }
            if (name != null) {
                Text(
                    name,
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontSize = TranscriptGrouping.Layout.NAME_SIZE.sp,
                        fontWeight = FontWeight.Medium,
                    ),
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(bottom = 2.dp).testTag("message-bubble-author"),
                )
            }
            content()
            val mark = status?.takeIf { own }
            val caption = listOfNotNull(
                time?.takeIf { it.isNotEmpty() },
                ChatUiCopy.QUEUED.takeIf { queued && mark == null },
            ).joinToString(" · ")
            if (caption.isNotEmpty() || onUnqueue != null || mark != null) {
                Row(
                    modifier = Modifier.align(Alignment.End).padding(top = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        caption,
                        style = MaterialTheme.typography.labelSmall,
                        color = BubbleMuted,
                        maxLines = 1,
                        modifier = Modifier.testTag(if (queued && mark == null) "chat-message-queued" else "chat-bubble-time"),
                    )
                    if (mark != null) MessageStatusMark(mark, onClick = onStatusClick)
                    if (onUnqueue != null) UnqueueButton(onUnqueue)
                }
            }
        }
    }
}

/** #FEED-237: genUI's `MessageStatus`: the state's concept icon at
 *  `markSize`, in its tone (muted = the time's ink, info = the read blue,
 *  danger = destructive), labelled `copy.status[state]`. */
@Composable
fun MessageStatusMark(state: MessageDelivery.State, modifier: Modifier = Modifier, onClick: (() -> Unit)? = null) {
    val tint = when (state.tone) {
        MessageDelivery.Tone.Muted -> BubbleMuted
        MessageDelivery.Tone.Info -> DesignTokens.Semantic.Blue
        MessageDelivery.Tone.Danger -> DesignTokens.Palette.Destructive
    }
    val icon = ExpIcons.byConcept(state.icon) ?: ExpIcons.uiCheck
    Icon(
        icon,
        contentDescription = state.label,
        tint = tint,
        modifier = modifier
            .padding(start = 4.dp)
            .size(MessageDelivery.MARK_SIZE.dp)
            .then(if (onClick != null) Modifier.clip(CircleShape).clickable(role = Role.Button, onClick = onClick) else Modifier)
            .testTag(MessageDelivery.TestIds.STATUS)
            .semantics { stateDescription = state.wire },
    )
}

/** #FEED-238: the transcript body's line height (MarkdownView's chat body). */
private const val TRANSCRIPT_LINE_SP = 22f

/** #FEED-238 THE FOLD's clip: `fold.lines` lines of the transcript body. */
@Composable
fun messageFoldHeight(): Dp = with(LocalDensity.current) { (TRANSCRIPT_LINE_SP * TranscriptGrouping.Fold.LINES).sp.toDp() }

/**
 * #FEED-238 THE FOLD ×4 (`messageFolds`): a message bubble's body whose
 * [text] folds shows its first `fold.lines` lines, then THE transcript's
 * Show more / Show less text button ([MessageFoldToggle]). Every message
 * bubble: own, member, agent, a relay, a child's report.
 */
@Composable
fun MessageFold(text: String, content: @Composable () -> Unit) {
    val folds = remember(text) { TranscriptGrouping.messageFolds(text) }
    var expanded by remember(text) { mutableStateOf(false) }
    val clip = messageFoldHeight()
    Column(if (folds && !expanded) Modifier.clipFoldTo(clip) else Modifier) { content() }
    if (folds) MessageFoldToggle(expanded) { expanded = !expanded }
}

/** THE transcript's Show more / Show less text button (`copy.showMore`). */
@Composable
fun MessageFoldToggle(expanded: Boolean, onToggle: () -> Unit) {
    Text(
        if (expanded) TranscriptGrouping.Copy.SHOW_LESS else TranscriptGrouping.Copy.SHOW_MORE,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier
            .clickable(onClick = onToggle)
            .padding(top = 2.dp)
            .testTag("message-fold-toggle"),
    )
}

/** The fold CLIPS the naturally laid-out body (never squeezes its measure). */
fun Modifier.clipFoldTo(clampHeight: Dp): Modifier =
    clipToBounds().layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
        val height = minOf(placeable.height, clampHeight.roundToPx())
        layout(placeable.width, height) { placeable.placeRelative(0, 0) }
    }

/** [MessageBubble] over markdown text (the conversation's rows), folded
 *  (#FEED-238). */
@Composable
fun MessageBubble(
    text: String,
    own: Boolean,
    time: String?,
    modifier: Modifier = Modifier,
    author: String? = null,
    onClick: (() -> Unit)? = null,
    queued: Boolean = false,
    onUnqueue: (() -> Unit)? = null,
    status: MessageDelivery.State? = null,
) {
    MessageBubble(
        own = own,
        time = time,
        modifier = modifier,
        author = author,
        onClick = onClick,
        queued = queued,
        onUnqueue = onUnqueue,
        status = status,
    ) {
        MessageFold(text) { MarkdownView(markdown = text, softBreaksAsNewlines = true) }
    }
}

/** The bubble's name line for a row: the group's first bubble only; a
 *  cross-session sender's ends with ` ›`. */
fun TranscriptGrouping.RowLayout?.authorLine(): String? =
    this?.takeIf { it.name }?.sender?.label

/** #EXP-1772: a relay = a row whose sender is another session (it links). */
fun TranscriptGrouping.RowLayout?.isRelay(): Boolean =
    this?.side == TranscriptGrouping.Side.Other && this.sender?.link != null

/**
 * #EXP-1772 (transcript-grouping.json RELAYS): another session's bubble.
 * Its name line ([author], `<name> ›`; the group's first relay bubble only)
 * sits ABOVE the bubble, outside it, in the in-bubble name line's style;
 * ONLY that line is the press ([onOpen]: the session on the back stack).
 * The bubble itself is never a link; #FEED-238: its body folds like every
 * bubble's (the caller wraps it in [MessageFold]).
 */
@Composable
fun RelayBubble(
    author: String?,
    time: String?,
    onOpen: (() -> Unit)?,
    modifier: Modifier = Modifier,
    queued: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        if (!author.isNullOrEmpty()) {
            Text(
                author,
                style = MaterialTheme.typography.labelSmall.copy(
                    fontSize = TranscriptGrouping.Layout.NAME_SIZE.sp,
                    fontWeight = FontWeight.Medium,
                ),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .padding(bottom = 2.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .then(if (onOpen != null) Modifier.clickable(role = Role.Button, onClick = onOpen) else Modifier)
                    .padding(horizontal = 2.dp)
                    .testTag("message-relay-author"),
            )
        }
        MessageBubble(own = false, time = time, queued = queued, content = content)
    }
}

/**
 * THE inline marker ×4 (the feed's `Context compacted` row): a centred
 * muted label, full width, never grouped. #EXP-1772: a system input (a
 * schedule or event firing, a PR or session status line), an `event` row
 * and a system message draw as it; [onClick] = a line about another session
 * (#EXP-1464: the trailing chevron, it opens that session).
 */
@Composable
fun FeedMarkerRow(
    label: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    onClick: (() -> Unit)? = null,
    testTag: String = "feed-marker",
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (onClick != null) {
                    Modifier.clip(RoundedCornerShape(6.dp)).clickable(role = Role.Button, onClick = onClick)
                } else {
                    Modifier
                },
            )
            .testTag(testTag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        // #EXP-1772: THE marker ×4, a centred label between two hairlines.
        val hairline = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f)
        Box(Modifier.weight(1f).height(1.dp).background(hairline))
        if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(12.dp), tint = muted)
        }
        Text(
            label,
            style = MaterialTheme.typography.labelMedium.copy(
                fontSize = DesignTokens.Transcript.ToolSize.sp,
                lineHeight = DesignTokens.Transcript.ToolLineHeight.sp,
            ),
            color = muted,
            // One line like web: the hairlines take the rest of the row.
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Box(Modifier.weight(1f).height(1.dp).background(hairline))
    }
}

/**
 * THE Avatar of a sender at its default size (`layout.avatar`): a member's
 * initials in their hue (their picture when synced), an agent's mark (the
 * synced definition through [LocalAgentMarks], else [agent]; the viewer's
 * Assistant wears the brand mark), a session's glyph on the muted disc (the
 * chat's own session with an [agent] = that agent's mark: an app editor).
 */
@Composable
fun SenderAvatar(
    sender: TranscriptGrouping.Sender,
    modifier: Modifier = Modifier,
    user: UserEntity? = null,
    agent: ChatAgent? = null,
) {
    val size = TranscriptGrouping.Layout.AVATAR.dp
    when (sender.kind) {
        TranscriptGrouping.SenderKind.Member ->
            UserAvatar(user = user, nameOrEmail = sender.name, modifier = modifier, size = size, userId = sender.id)
        TranscriptGrouping.SenderKind.Agent -> {
            val mark = LocalAgentMarks.current[sender.id]
            val own = agent?.takeIf { sender.link == null }
            if (mark?.isAssistant == true || (mark == null && own?.assistant == true)) {
                Box(
                    modifier = modifier
                        .size(size)
                        .clip(CircleShape)
                        .background(DesignTokens.Palette.Muted, CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    ExponentialMark(size = 18.dp, tint = MaterialTheme.colorScheme.onSurface)
                }
            } else {
                AgentAvatar(
                    icon = mark?.icon ?: own?.icon,
                    color = mark?.color ?: own?.color,
                    modifier = modifier,
                    size = size,
                )
            }
        }
        // An app editor's own session wears the app's mark ([agent]).
        TranscriptGrouping.SenderKind.Session -> if (agent != null && sender.link == null) {
            AgentAvatar(icon = agent.icon, color = agent.color, modifier = modifier, size = size)
        } else {
            SessionAvatar(modifier = modifier)
        }
    }
}

/** A session as a sender: its glyph on the muted disc. */
@Composable
private fun SessionAvatar(modifier: Modifier = Modifier) {
    val size = TranscriptGrouping.Layout.AVATAR.dp
    Box(
        modifier = modifier
            .size(size)
            .clip(CircleShape)
            .background(DesignTokens.Palette.Muted, CircleShape)
            .testTag("session-avatar"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.codingAssistant,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.size(16.dp),
        )
    }
}

/**
 * One transcript row: the gap above it ([TranscriptGrouping.RowLayout.gap]),
 * then `full` across the column, `other` under its sender's avatar
 * column (the avatar on the group's first row, top-aligned, `avatarGap` left
 * of the bubble column; the same space below it). #FEED-239: `own` mirrors
 * it on the RIGHT: the bubble column, `avatarGap`, the viewer's own avatar
 * column ([ownAvatar], default [ViewerAvatar]). A null [layout] = a full
 * row with no gap.
 */
@Composable
fun TranscriptBubbleRow(
    layout: TranscriptGrouping.RowLayout?,
    modifier: Modifier = Modifier,
    avatar: @Composable () -> Unit = {},
    ownAvatar: @Composable () -> Unit = { ViewerAvatar() },
    content: @Composable () -> Unit,
) {
    val top = (layout?.gap ?: 0).dp
    if (layout?.side == TranscriptGrouping.Side.Own) {
        Row(
            modifier = modifier.fillMaxWidth().padding(top = top),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.End,
        ) {
            Column(Modifier.weight(1f), horizontalAlignment = Alignment.End) { content() }
            Spacer(Modifier.width(TranscriptGrouping.Layout.AVATAR_GAP.dp))
            Box(Modifier.width(TranscriptGrouping.Layout.AVATAR.dp)) {
                if (layout.avatar) ownAvatar()
            }
        }
        return
    }
    if (layout == null || layout.side != TranscriptGrouping.Side.Other) {
        Box(modifier = modifier.fillMaxWidth().padding(top = top)) { content() }
        return
    }
    Row(
        modifier = modifier.fillMaxWidth().padding(top = top),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.Start,
    ) {
        Box(Modifier.width(TranscriptGrouping.Layout.AVATAR.dp)) {
            if (layout.avatar) avatar()
        }
        Spacer(Modifier.width(TranscriptGrouping.Layout.AVATAR_GAP.dp))
        Column(Modifier.weight(1f), horizontalAlignment = Alignment.Start) { content() }
    }
}

/** #FEED-239: the transcript's viewer (their synced row); null = none known
 *  (no own avatar then). */
val LocalViewer = androidx.compose.runtime.compositionLocalOf<UserEntity?> { null }

/**
 * #FEED-239: THE Avatar of the VIEWER at `layout.avatar` (their picture,
 * else their initials in their hue; THE [UserAvatar] every member wears),
 * right of their own bubbles. Draws nothing while the viewer is unknown.
 */
@Composable
fun ViewerAvatar(modifier: Modifier = Modifier) {
    val viewer = LocalViewer.current ?: return
    UserAvatar(
        user = viewer,
        nameOrEmail = viewer.name?.takeIf { it.isNotBlank() } ?: viewer.email,
        modifier = modifier.testTag("message-own-avatar"),
        size = TranscriptGrouping.Layout.AVATAR.dp,
        userId = viewer.id,
    )
}
