package com.exponential.app.ui.markdown

import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.FormatBold
import androidx.compose.material.icons.filled.FormatItalic
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.FormatListNumbered
import androidx.compose.material.icons.filled.FormatUnderlined
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Link
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import com.mohamedrejeb.richeditor.model.RichTextState
import kotlinx.coroutines.launch

private val BoldStyle = SpanStyle(fontWeight = FontWeight.Bold)
private val ItalicStyle = SpanStyle(fontStyle = FontStyle.Italic)
private val UnderlineStyle = SpanStyle(textDecoration = TextDecoration.Underline)

@Composable
fun MarkdownToolbar(
    state: RichTextState,
    onUploadImage: (suspend (Uri) -> String?)?,
    imageUploadEnabled: Boolean,
) {
    val scope = rememberCoroutineScope()

    var linkDialogOpen by remember { mutableStateOf(false) }
    var uploading by remember { mutableStateOf(false) }

    val pickImage = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia(),
    ) { uri ->
        if (uri == null || onUploadImage == null) return@rememberLauncherForActivityResult
        scope.launch {
            uploading = true
            try {
                val url = onUploadImage(uri)
                if (url != null) {
                    val alt = uri.lastPathSegment?.substringAfterLast('/') ?: "image"
                    state.insertMarkdownAfterSelection("\n\n![${alt}](${url})\n\n")
                }
            } finally {
                uploading = false
            }
        }
    }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        ToolbarToggle(
            icon = Icons.Filled.FormatBold,
            label = "Bold",
            active = state.currentSpanStyle.fontWeight == FontWeight.Bold,
            onClick = { state.toggleSpanStyle(BoldStyle) },
        )
        ToolbarToggle(
            icon = Icons.Filled.FormatItalic,
            label = "Italic",
            active = state.currentSpanStyle.fontStyle == FontStyle.Italic,
            onClick = { state.toggleSpanStyle(ItalicStyle) },
        )
        ToolbarToggle(
            icon = Icons.Filled.FormatUnderlined,
            label = "Underline",
            active = state.currentSpanStyle.textDecoration == TextDecoration.Underline,
            onClick = { state.toggleSpanStyle(UnderlineStyle) },
        )
        Spacer(Modifier.width(4.dp))
        HeadingButton(state, prefix = "# ", label = "H1")
        HeadingButton(state, prefix = "## ", label = "H2")
        HeadingButton(state, prefix = "### ", label = "H3")
        Spacer(Modifier.width(4.dp))
        ToolbarToggle(
            icon = Icons.AutoMirrored.Filled.FormatListBulleted,
            label = "Bullet list",
            active = state.isUnorderedList,
            onClick = { state.toggleUnorderedList() },
        )
        ToolbarToggle(
            icon = Icons.Filled.FormatListNumbered,
            label = "Numbered list",
            active = state.isOrderedList,
            onClick = { state.toggleOrderedList() },
        )
        ToolbarToggle(
            icon = Icons.Filled.Code,
            label = "Code",
            active = state.isCodeSpan,
            onClick = { state.toggleCodeSpan() },
        )
        Spacer(Modifier.width(4.dp))
        ToolbarToggle(
            icon = Icons.Filled.Link,
            label = "Link",
            active = state.isLink,
            onClick = { linkDialogOpen = true },
        )
        if (onUploadImage != null) {
            ToolbarToggle(
                icon = Icons.Filled.Image,
                label = "Image",
                active = uploading,
                enabled = imageUploadEnabled && !uploading,
                onClick = {
                    pickImage.launch(
                        PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                    )
                },
            )
        }
    }

    if (linkDialogOpen) {
        InsertLinkDialog(
            initialText = state.selectedLinkText.orEmpty(),
            initialUrl = state.selectedLinkUrl.orEmpty(),
            onInsert = { text, url ->
                if (url.isNotBlank()) {
                    state.addLink(text = text.ifBlank { url }, url = url)
                }
                linkDialogOpen = false
            },
            onDismiss = { linkDialogOpen = false },
        )
    }
}

@Composable
private fun HeadingButton(state: RichTextState, prefix: String, label: String) {
    IconButton(onClick = { state.insertMarkdownAfterSelection("\n\n$prefix") }) {
        Text(label, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
private fun ToolbarToggle(
    icon: ImageVector,
    label: String,
    active: Boolean,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        enabled = enabled,
        colors = if (active) {
            IconButtonDefaults.iconButtonColors(
                containerColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.18f),
            )
        } else IconButtonDefaults.iconButtonColors(),
    ) {
        Icon(icon, contentDescription = label, modifier = Modifier.size(18.dp))
    }
}

@Composable
private fun InsertLinkDialog(
    initialText: String,
    initialUrl: String,
    onInsert: (text: String, url: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var text by remember { mutableStateOf(initialText) }
    var url by remember { mutableStateOf(initialUrl) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Insert link") },
        text = {
            Column {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Display text") },
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
            TextButton(onClick = { onInsert(text, url) }, enabled = url.isNotBlank()) {
                Text("Insert")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

internal fun guessMimeType(context: android.content.Context, uri: Uri, fallback: String = "image/jpeg"): String {
    val resolver = context.contentResolver
    val type = resolver.getType(uri)
    if (!type.isNullOrBlank()) return type
    val ext = MimeTypeMap.getFileExtensionFromUrl(uri.toString())
    if (!ext.isNullOrBlank()) {
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)?.let { return it }
    }
    return fallback
}

internal fun guessFilename(context: android.content.Context, uri: Uri): String {
    val resolver = context.contentResolver
    resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
        if (cursor.moveToFirst() && idx >= 0) {
            val name = cursor.getString(idx)
            if (!name.isNullOrBlank()) return name
        }
    }
    return uri.lastPathSegment ?: "image"
}
