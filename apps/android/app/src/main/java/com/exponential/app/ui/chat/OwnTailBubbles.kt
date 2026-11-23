package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.session.LocalFindBubbleLanding
import com.exponential.app.ui.session.LocalFindPaint
import com.exponential.app.ui.session.findRing
import com.exponential.app.ui.session.ownBubbleFindKey

/**
 * #FEED-237 (message-delivery.json): the viewer's OWN bubbles at the bottom
 * of a session's transcript: the consumed rows whose echo has not reached
 * the transcript yet (HANDOFF, `read`), their synced rows the device has not
 * consumed, then the outbox entries, each with its delivery mark beside the
 * `HH:mm`. Keyed by message id, so a row keeps one bubble across states.
 * A long-press opens the bubble's menu (`bubbleActions`: Retry + Delete on a
 * failed one, Cancel sending on a stored one), a failed mark opens it too.
 * #EXP-1975 (find-in-chat.json): each bubble paints find in chat's marks
 * ([LocalFindPaint], keyed by [ownBubbleFindKey]).
 */
@Composable
fun OwnTailBubbles(
    bubbles: List<MessageDelivery.Bubble>,
    onAction: (MessageDelivery.Bubble, MessageDelivery.Action) -> Unit,
    modifier: Modifier = Modifier,
    clock: (wire: String) -> String? = ::clockTime,
    /** #EXP-1876: these bubbles open a new own group (nothing own right
     *  above them), so the first wears THE tail, as on iOS and the web;
     *  #EXP-1854: and the viewer's avatar (no transcript row carries it). */
    startsGroup: Boolean = true,
    /** #EXP-1990: the viewer, and the team's members by id: a TEAMMATE's
     *  sent input ([MessageDelivery.Bubble.authorId], a team agent's group
     *  chat) sits left under their avatar and name, through THE grouping. */
    viewerId: String? = null,
    members: Map<String, com.exponential.app.data.db.UserEntity> = emptyMap(),
) {
    if (bubbles.isEmpty()) return
    val layouts = remember(bubbles, viewerId, members) {
        sentInputLayouts(bubbles, viewerId) { id -> members[id]?.name?.takeIf { it.isNotBlank() } }
    }
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        bubbles.forEachIndexed { index, bubble ->
            androidx.compose.runtime.key(bubble.key) {
                val layout = layouts?.get(bubble.key)
                if (layout == null) {
                    OwnTailBubble(bubble, onAction, clock, tail = startsGroup && index == 0, findKey = ownBubbleFindKey(bubble))
                } else if (layout.side == TranscriptGrouping.Side.Other) {
                    // A teammate's: THE bubble under their avatar and name.
                    val sender = layout.sender
                    TranscriptBubbleRow(
                        layout = layout.copy(gap = groupGap(layout, index)),
                        avatar = { if (sender != null) SenderAvatar(sender, user = members[sender.id]) },
                    ) {
                        MessageBubble(
                            own = false,
                            time = clock(bubble.createdAt),
                            author = layout.authorLine(),
                            testTag = "chat-member-sent",
                            tail = layout.hasTail(),
                        ) {
                            if (bubble.text.isNotBlank()) {
                                MarkdownView(
                                    markdown = bubble.text,
                                    softBreaksAsNewlines = true,
                                    highlight = LocalFindPaint.current?.highlight(ownBubbleFindKey(bubble)),
                                )
                            }
                        }
                    }
                } else {
                    Box(Modifier.padding(top = groupGap(layout, index).dp)) {
                        OwnTailBubble(bubble, onAction, clock, tail = layout.tail, findKey = ownBubbleFindKey(bubble))
                    }
                }
            }
        }
    }
}

/** The extra space above a row that opens a new sender group (the column
 *  already spaces every row by the in-group gap). */
private fun groupGap(layout: TranscriptGrouping.RowLayout, index: Int): Int =
    if (index == 0) 0 else (layout.gap - TranscriptGrouping.Layout.GAP_IN_GROUP).coerceAtLeast(0)

/**
 * #EXP-1990 (transcript-grouping.json): the sent inputs through THE grouping
 * by their synced author: the viewer's own group right, a teammate's (a team
 * agent's group chat) left under their avatar and name; by bubble key. Null
 * = every bubble is the viewer's (the own tail as before).
 */
internal fun sentInputLayouts(
    bubbles: List<MessageDelivery.Bubble>,
    viewerId: String?,
    memberName: (userId: String) -> String?,
): Map<String, TranscriptGrouping.RowLayout>? {
    if (viewerId.isNullOrEmpty() || bubbles.none { !it.authorId.isNullOrEmpty() && it.authorId != viewerId }) return null
    val items = bubbles.map {
        TranscriptGrouping.Item(
            it.key,
            TranscriptGrouping.ItemKind.Message,
            authorKind = com.exponential.app.domain.DomainContract.messageAuthorKindUser,
            authorId = it.authorId?.takeIf { id -> id.isNotEmpty() } ?: viewerId,
        )
    }
    val members = items.mapNotNull { it.authorId }.distinct().filter { it != viewerId }
        .mapNotNull { id -> memberName(id)?.let { TranscriptGrouping.Named(id, it) } }
    val layouts = TranscriptGrouping.transcriptLayout(
        TranscriptGrouping.Input(
            viewerId = viewerId,
            chat = TranscriptGrouping.Chat(sessionId = "", title = "", agentId = null, originSessionId = null, fromStart = false),
            members = members,
            items = items,
        ),
    )
    return layouts.associateBy { it.id }
}

@Composable
private fun OwnTailBubble(
    bubble: MessageDelivery.Bubble,
    onAction: (MessageDelivery.Bubble, MessageDelivery.Action) -> Unit,
    clock: (wire: String) -> String?,
    tail: Boolean,
    findKey: String,
) {
    val highlight = LocalFindPaint.current?.highlight(findKey)
    // #EXP-1998: under the no-transcript placeholder find in chat centres
    // and rings a bubble.
    val landing = LocalFindBubbleLanding.current
    val actions = bubble.actions
    var open by remember { mutableStateOf(false) }
    // #FEED-239: aligned with the transcript's own bubbles, the viewer's
    // avatar column on the right; #EXP-1854: a bubble that opens the own
    // group ([tail]) wears the viewer's avatar there, as on iOS, the web and
    // the desktop (the no-feed placeholder has no transcript row to carry it).
    Row(
        if (landing == null) {
            Modifier.fillMaxWidth()
        } else {
            Modifier.fillMaxWidth().onGloballyPositioned { landing.onPlaced(findKey, it) }.findRing(landing.ring == findKey)
        },
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.weight(1f)) {
            MessageBubble(
                own = true,
                time = clock(bubble.createdAt),
                status = bubble.state,
                onLongClick = if (actions.isNotEmpty()) ({ open = true }) else null,
                onStatusClick = if (bubble.state == MessageDelivery.State.Failed) ({ open = true }) else null,
                testTag = "chat-own-pending",
                tail = tail,
            ) {
                if (bubble.text.isNotBlank()) MarkdownView(markdown = bubble.text, softBreaksAsNewlines = true, highlight = highlight)
            }
            // The menu anchors at the bubble's bottom-right corner.
            Box(Modifier.align(Alignment.BottomEnd)) {
                GlassDropdownMenu(
                    expanded = open && actions.isNotEmpty(),
                    onDismissRequest = { open = false },
                    modifier = Modifier.testTag(MessageDelivery.TestIds.MENU),
                ) {
                    for (action in actions) {
                        GlassMenuItem(
                            text = { Text(action.label) },
                            onClick = {
                                open = false
                                onAction(bubble, action)
                            },
                            destructive = action == MessageDelivery.Action.Delete,
                            modifier = Modifier.testTag("message-menu-${action.wire}"),
                        )
                    }
                }
            }
        }
        Spacer(Modifier.width(TranscriptGrouping.Layout.AVATAR_GAP.dp))
        Box(Modifier.width(TranscriptGrouping.Layout.AVATAR.dp)) {
            if (tail) ViewerAvatar()
        }
    }
}
