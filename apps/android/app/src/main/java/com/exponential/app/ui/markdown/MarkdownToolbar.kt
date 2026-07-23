package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.Checklist
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.FormatListNumbered
import androidx.compose.material.icons.filled.FormatQuote
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ListType

private val PillBg = Color.White.copy(alpha = 0.10f)
private val IconInactive = Color.White.copy(alpha = 0.65f)
private val SepColor = Color.White.copy(alpha = 0.12f)

/**
 * Formatting bar driving the [EditorModel] — the Linear-style simplified set
 * (EXP-246): image, @/# autocomplete triggers, lists, code block and quote.
 * The inline-mark affordances (heading/bold/italic/strikethrough/link) were
 * removed; existing marks still render and round-trip, they just aren't
 * authorable from the bar. Block buttons act on the active row; @/# insert
 * their trigger character at the caret so the editor's autocomplete popup opens.
 */
@Composable
fun MarkdownToolbar(
    model: EditorModel,
    onPickImage: () -> Unit,
    imageEnabled: Boolean,
    mentionEnabled: Boolean = true,
    modifier: Modifier = Modifier,
) {
    val activeRowId = model.activeRowId
    val attrs = activeRowId?.let { model.attrsFor(it) }

    Column(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 6.dp, vertical = 4.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(PillBg),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 4.dp, vertical = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(1.dp),
            ) {
                ToolbarButton(Icons.Filled.Image, "Image", active = false, enabled = imageEnabled) { onPickImage() }
                Separator()
                if (mentionEnabled) {
                    ToolbarButton(Icons.Filled.AlternateEmail, "Mention a member", active = false) {
                        model.insertPlainText("@")
                    }
                }
                ToolbarButton(Icons.Filled.Tag, "Reference an issue", active = false) {
                    model.insertPlainText("#")
                }
                Separator()
                ToolbarButton(Icons.AutoMirrored.Filled.FormatListBulleted, "Bullet list", active = attrs?.listType == ListType.Bullet) {
                    activeRowId?.let { model.toggleList(it, ListType.Bullet) }
                }
                ToolbarButton(Icons.Filled.FormatListNumbered, "Numbered list", active = attrs?.listType == ListType.Ordered) {
                    activeRowId?.let { model.toggleList(it, ListType.Ordered) }
                }
                ToolbarButton(Icons.Filled.Checklist, "Task list", active = attrs?.listType == ListType.Checklist) {
                    activeRowId?.let { model.toggleList(it, ListType.Checklist) }
                }
                ToolbarButton(Icons.Filled.Code, "Code block", active = attrs?.kind == BlockKind.CodeBlock) {
                    activeRowId?.let { model.toggleCodeBlock(it) }
                }
                ToolbarButton(Icons.Filled.FormatQuote, "Quote", active = attrs?.kind == BlockKind.Blockquote) {
                    activeRowId?.let { model.toggleQuote(it) }
                }
            }
        }
    }
}

@Composable
private fun ToolbarButton(
    icon: ImageVector,
    label: String,
    active: Boolean,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, enabled = enabled, modifier = Modifier.size(36.dp)) {
        Icon(
            icon,
            contentDescription = label,
            tint = if (!enabled) Color.White.copy(alpha = 0.3f) else if (active) MdStyle.Link else IconInactive,
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun Separator() {
    Spacer(
        modifier = Modifier
            .padding(horizontal = 3.dp)
            .width(1.dp)
            .height(18.dp)
            .background(SepColor),
    )
}
