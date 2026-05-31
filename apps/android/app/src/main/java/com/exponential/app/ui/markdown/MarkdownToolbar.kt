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
import androidx.compose.material.icons.filled.Checklist
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.FormatBold
import androidx.compose.material.icons.filled.FormatItalic
import androidx.compose.material.icons.filled.FormatListNumbered
import androidx.compose.material.icons.filled.FormatQuote
import androidx.compose.material.icons.filled.FormatSize
import androidx.compose.material.icons.filled.FormatStrikethrough
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.KeyboardHide
import androidx.compose.material.icons.filled.Link
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType

private val PillBg = Color.White.copy(alpha = 0.10f)
private val IconInactive = Color.White.copy(alpha = 0.65f)
private val SepColor = Color.White.copy(alpha = 0.12f)

/**
 * Formatting bar driving the [EditorModel] — ports the iOS `MarkdownToolbar`
 * button set, order and active-tint styling (active = link blue, no fill). Mark
 * buttons act on the active row's selection; block buttons act on the active row.
 */
@Composable
fun MarkdownToolbar(
    model: EditorModel,
    onPickImage: () -> Unit,
    imageEnabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val keyboard = LocalSoftwareKeyboardController.current
    var linkDialogOpen by remember { mutableStateOf(false) }

    val activeRowId = model.activeRowId
    val attrs = activeRowId?.let { model.attrsFor(it) }
    val sel = model.activeSelection()
    val hasSelection = sel != null && sel.second.last > sel.second.first

    fun markActive(kind: InlineKind): Boolean {
        val s = sel ?: return false
        if (s.second.last <= s.second.first) return false
        return MarkOps.hasMarkOver(model.marksFor(s.first), s.second.first, s.second.last, kind)
    }

    fun toggleMark(kind: InlineKind) {
        val s = sel ?: return
        if (s.second.last > s.second.first) model.toggleMark(s.first, s.second, kind)
    }

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
                    .padding(end = 40.dp) // leave room for the pinned dismiss button
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 4.dp, vertical = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(1.dp),
            ) {
                ToolbarButton(Icons.Filled.Image, "Image", active = false, enabled = imageEnabled) { onPickImage() }
                Separator()
                ToolbarButton(Icons.Filled.FormatSize, "Heading", active = attrs?.kind == BlockKind.Heading) {
                    activeRowId?.let { model.cycleHeading(it) }
                }
                ToolbarButton(Icons.Filled.FormatBold, "Bold", active = markActive(InlineKind.Bold), enabled = hasSelection) {
                    toggleMark(InlineKind.Bold)
                }
                ToolbarButton(Icons.Filled.FormatItalic, "Italic", active = markActive(InlineKind.Italic), enabled = hasSelection) {
                    toggleMark(InlineKind.Italic)
                }
                ToolbarButton(Icons.Filled.FormatStrikethrough, "Strikethrough", active = markActive(InlineKind.Strikethrough), enabled = hasSelection) {
                    toggleMark(InlineKind.Strikethrough)
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
                Separator()
                ToolbarButton(Icons.Filled.Link, "Link", active = markActive(InlineKind.Link)) {
                    linkDialogOpen = true
                }
            }
            // Pinned keyboard-dismiss button at the right edge.
            Box(modifier = Modifier.align(Alignment.CenterEnd)) {
                ToolbarButton(Icons.Filled.KeyboardHide, "Hide keyboard", active = false) {
                    model.setFocused(null)
                    keyboard?.hide()
                }
            }
        }
    }

    if (linkDialogOpen) {
        InsertLinkDialog(
            onInsert = { text, url ->
                linkDialogOpen = false
                val s = sel
                val normalized = if (url.contains("://")) url else "https://$url"
                if (s != null && s.second.last > s.second.first) {
                    model.toggleMark(s.first, s.second, InlineKind.Link, normalized)
                } else if (s != null) {
                    model.insertLinkText(s.first, s.second.first, text.ifBlank { normalized }, normalized)
                }
            },
            onDismiss = { linkDialogOpen = false },
        )
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
            tint = if (!enabled) IconInactive.copy(alpha = 0.3f) else if (active) MdStyle.Link else IconInactive,
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

@Composable
private fun InsertLinkDialog(
    onInsert: (text: String, url: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var text by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Insert link") },
        text = {
            Column {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Display text (optional)") },
                    singleLine = true,
                )
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text("URL") },
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onInsert(text, url) }, enabled = url.isNotBlank()) { Text("Insert") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
