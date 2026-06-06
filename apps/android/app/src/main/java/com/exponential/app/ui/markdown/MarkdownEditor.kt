package com.exponential.app.ui.markdown

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import android.content.Context
import com.exponential.app.ui.markdown.model.PendingImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Block-based markdown editor / viewer. In `editable` mode it renders the
 * [EditorModel]'s rows as per-paragraph fields plus a formatting toolbar; in
 * read mode it delegates to [MarkdownView]. The public signature is unchanged
 * from the previous `compose-rich-editor` wrapper so all call sites compile as-is:
 * markdown flows in via [markdown] and out via [onChange] (callers debounce).
 *
 * `onUploadImage` keeps its contract: it returns a real `/api/attachments/...`
 * URL (issue detail, eager upload) or a `draft://` placeholder (create sheet,
 * deferred upload). Either way the returned URL is inserted as an image block;
 * draft images preview from the locally-read bytes.
 */
@Composable
fun MarkdownEditor(
    markdown: String,
    editable: Boolean,
    onChange: (String) -> Unit,
    onUploadImage: (suspend (uri: Uri) -> String?)? = null,
    imageUploadEnabled: Boolean = onUploadImage != null,
    placeholder: String = "Add a description…",
    minHeight: Dp = 200.dp,
    // Preview bytes for draft images already embedded in [markdown] (e.g. content
    // shared into the app). Keyed by the same `draft://` placeholder that appears
    // in the markdown so the tiles render before the host uploads them.
    initialPendingImages: Map<String, Uri> = emptyMap(),
    // Workspace members offered by @mention autocomplete (agents excluded by the
    // caller). Empty disables the affordance.
    mentionMembers: List<MentionMember> = emptyList(),
    modifier: Modifier = Modifier,
) {
    if (!editable) {
        if (markdown.isBlank()) {
            Box(modifier = modifier.padding(vertical = 8.dp)) {
                Text(
                    "No description",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        } else {
            MarkdownView(markdown, modifier = modifier)
        }
        return
    }

    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val model = remember { EditorModel() }
    val currentOnChange by rememberUpdatedState(onChange)
    val currentUploader by rememberUpdatedState(onUploadImage)
    val currentInitialPending by rememberUpdatedState(initialPendingImages)

    // Wire edits → markdown out (once; closure reads the latest onChange).
    LaunchedEffect(model) {
        model.onEdit = { currentOnChange(model.currentMarkdown()) }
    }

    // Load external markdown only when it actually differs from what we derive,
    // so the user's own keystrokes (which already emitted this value) never reload.
    LaunchedEffect(markdown) {
        if (model.currentMarkdown() != markdown) {
            model.load(markdown)
            // load() clears pendingImages, so re-seed preview bytes for any draft
            // images carried in via [initialPendingImages] (shared content).
            seedPendingPreviews(context, model, currentInitialPending)
        }
    }

    val pickImage = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia(),
    ) { uri: Uri? ->
        val uploader = currentUploader
        if (uri == null || uploader == null) return@rememberLauncherForActivityResult
        scope.launch {
            val bytes = MarkdownMediaUtils.readBytes(context, uri)
            val mime = MarkdownMediaUtils.guessMimeType(context, uri)
            val name = MarkdownMediaUtils.guessFilename(context, uri)
            val size = MarkdownMediaUtils.probeSize(context, uri)
            val url = runCatching { uploader(uri) }.getOrNull() ?: return@launch
            val pending = if (bytes != null) {
                PendingImage(uri, bytes, name, mime, size.width, size.height)
            } else null
            model.insertImageUrl(url, alt = "image", pending = pending)
        }
    }

    Column(modifier = modifier.fillMaxWidth()) {
        MarkdownToolbar(
            model = model,
            onPickImage = {
                pickImage.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            },
            imageEnabled = imageUploadEnabled && onUploadImage != null,
        )
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = minHeight)
                .padding(top = 8.dp),
        ) {
            val rows = model.rows
            val soleEmptyId = rows.singleOrNull()?.let { (it as? EditorRow.Para)?.takeIf { p -> p.text.isEmpty() }?.id }
            rows.forEach { row ->
                key(row.id) {
                    when (row) {
                        is EditorRow.Para -> BlockTextField(
                            model = model,
                            row = row,
                            placeholder = if (row.id == soleEmptyId) placeholder else null,
                            mentionMembers = mentionMembers,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        is EditorRow.Image -> BlockImageEditView(model = model, row = row)
                    }
                }
            }
        }
    }
}

/**
 * Register preview bytes for prefilled draft images so their tiles render before
 * the host uploads them. Reads each cached image off the IO dispatcher, then
 * publishes the [PendingImage] into the (snapshot-state) map on the caller's
 * dispatcher.
 */
private suspend fun seedPendingPreviews(
    context: Context,
    model: EditorModel,
    pending: Map<String, Uri>,
) {
    if (pending.isEmpty()) return
    for ((placeholder, uri) in pending) {
        if (model.pendingImages[placeholder] != null) continue
        val image = withContext(Dispatchers.IO) {
            val bytes = MarkdownMediaUtils.readBytes(context, uri) ?: return@withContext null
            val mime = MarkdownMediaUtils.guessMimeType(context, uri)
            val name = MarkdownMediaUtils.guessFilename(context, uri)
            val size = MarkdownMediaUtils.probeSize(context, uri)
            PendingImage(uri, bytes, name, mime, size.width, size.height)
        } ?: continue
        model.pendingImages[placeholder] = image
    }
}

/** A workspace member offered by @mention autocomplete. */
data class MentionMember(val name: String, val email: String)

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
