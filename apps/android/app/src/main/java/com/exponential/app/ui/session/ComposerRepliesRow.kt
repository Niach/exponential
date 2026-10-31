package com.exponential.app.ui.session

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1211 ×4: the quick replies directly above THE session composer
 * (session-thread.json `copy.replies`): one small action [GlassPill] per
 * reply (single line, ellipsized), scrolling sideways on a phone, and a
 * trailing ghost dismiss (`ui-close`). A tap sends that reply as the next
 * message; [pending] disables the row while a call is in flight.
 */
@Composable
internal fun ComposerRepliesRow(
    replies: SessionTree.ComposerReplies,
    pending: Boolean,
    onSend: (itemId: String) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag(SessionTree.RepliesCopy.TEST_ID)
            .semantics { contentDescription = SessionTree.RepliesCopy.LABEL },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            for (item in replies.items) {
                GlassPill(
                    label = item.text,
                    onClick = { onSend(item.id) },
                    size = PillSize.Sm,
                    mode = PillMode.Action,
                    enabled = !pending,
                    // Floats over the transcript: the opaque card under the glass.
                    opaque = true,
                    maxLines = 1,
                    modifier = Modifier.widthIn(max = 240.dp),
                )
            }
        }
        IconButton(
            onClick = onDismiss,
            enabled = !pending,
            modifier = Modifier.size(28.dp).testTag("thread-replies-dismiss"),
        ) {
            Icon(
                imageVector = ExpIcons.uiClose,
                contentDescription = SessionTree.RepliesCopy.DISMISS,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}
