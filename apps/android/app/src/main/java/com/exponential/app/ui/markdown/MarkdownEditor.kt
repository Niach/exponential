package com.exponential.app.ui.markdown

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.mohamedrejeb.richeditor.model.RichTextState
import com.mohamedrejeb.richeditor.model.rememberRichTextState
import com.mohamedrejeb.richeditor.ui.material3.OutlinedRichTextEditor
import com.mohamedrejeb.richeditor.ui.material3.RichText
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * Wrapper around `compose-rich-editor` that round-trips markdown to/from
 * `RichTextState`. Same composable handles edit and read modes.
 */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun MarkdownEditor(
    markdown: String,
    editable: Boolean,
    onChange: (String) -> Unit,
    onUploadImage: (suspend (uri: android.net.Uri) -> String?)? = null,
    imageUploadEnabled: Boolean = onUploadImage != null,
    placeholder: String = "Add a description…",
    minHeight: androidx.compose.ui.unit.Dp = 200.dp,
    modifier: Modifier = Modifier,
) {
    val state = rememberRichTextState()

    // Load markdown once per source change (avoid re-loading while user is editing).
    LaunchedEffect(markdown) {
        if (state.toMarkdown() != markdown) {
            state.setMarkdown(markdown)
        }
    }

    // Emit markdown updates to the parent on text changes.
    LaunchedEffect(state, editable) {
        if (!editable) return@LaunchedEffect
        snapshotFlow { state.annotatedString }
            .distinctUntilChanged()
            .collect {
                val current = state.toMarkdown()
                if (current != markdown) onChange(current)
            }
    }

    Column(modifier = modifier.fillMaxWidth()) {
        if (editable) {
            MarkdownToolbar(
                state = state,
                onUploadImage = onUploadImage,
                imageUploadEnabled = imageUploadEnabled,
            )
            OutlinedRichTextEditor(
                state = state,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = minHeight)
                    .padding(top = 8.dp),
                placeholder = { Text(placeholder, color = MaterialTheme.colorScheme.onSurfaceVariant) },
            )
        } else if (markdown.isBlank()) {
            Box(modifier = Modifier.padding(vertical = 8.dp)) {
                Text(
                    "No description",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        } else {
            RichText(
                state = state,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** Pull `text` out of `{ "text": "..." }` issue description JSON; tolerate plain markdown. */
fun extractDescriptionMarkdown(raw: String?): String {
    if (raw.isNullOrBlank()) return ""
    return runCatching {
        val element = kotlinx.serialization.json.Json.parseToJsonElement(raw)
        if (element is kotlinx.serialization.json.JsonObject) {
            (element["text"] as? kotlinx.serialization.json.JsonPrimitive)?.content ?: raw
        } else raw
    }.getOrDefault(raw)
}

@Suppress("unused")
private fun stableTouch(state: RichTextState) {
    // Keep import live for IDE; intentionally unused.
}
