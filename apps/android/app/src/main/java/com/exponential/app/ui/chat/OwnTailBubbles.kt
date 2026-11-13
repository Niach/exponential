package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.markdown.MarkdownView

/**
 * #FEED-237 (message-delivery.json): the viewer's OWN bubbles at the bottom
 * of a session's transcript: the consumed rows whose echo has not reached
 * the transcript yet (HANDOFF, `read`), their synced rows the device has not
 * consumed, then the outbox entries, each with its delivery mark beside the
 * `HH:mm`. Keyed by message id, so a row keeps one bubble across states.
 * A long-press opens the bubble's menu (`bubbleActions`: Retry + Delete on a
 * failed one, Cancel sending on a stored one), a failed mark opens it too.
 */
@Composable
fun OwnTailBubbles(
    bubbles: List<MessageDelivery.Bubble>,
    onAction: (MessageDelivery.Bubble, MessageDelivery.Action) -> Unit,
    modifier: Modifier = Modifier,
    clock: (wire: String) -> String? = ::clockTime,
) {
    if (bubbles.isEmpty()) return
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        for (bubble in bubbles) {
            androidx.compose.runtime.key(bubble.key) {
                OwnTailBubble(bubble, onAction, clock)
            }
        }
    }
}

@Composable
private fun OwnTailBubble(
    bubble: MessageDelivery.Bubble,
    onAction: (MessageDelivery.Bubble, MessageDelivery.Action) -> Unit,
    clock: (wire: String) -> String?,
) {
    val actions = bubble.actions
    var open by remember { mutableStateOf(false) }
    Box(modifier = Modifier.fillMaxWidth()) {
        MessageBubble(
            own = true,
            time = clock(bubble.createdAt),
            status = bubble.state,
            onLongClick = if (actions.isNotEmpty()) ({ open = true }) else null,
            onStatusClick = if (bubble.state == MessageDelivery.State.Failed) ({ open = true }) else null,
            testTag = "chat-own-pending",
        ) {
            if (bubble.text.isNotBlank()) MarkdownView(markdown = bubble.text, softBreaksAsNewlines = true)
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
}
