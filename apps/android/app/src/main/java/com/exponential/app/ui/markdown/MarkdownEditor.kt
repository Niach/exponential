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
import androidx.compose.runtime.DisposableEffect
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
    // Team members offered by @mention autocomplete (agents excluded by the
    // caller). Empty disables the affordance.
    mentionMembers: List<MentionMember> = emptyList(),
    // Reports whether any field of this editor holds focus. Lets the host gate a
    // live remote-apply on "not currently editing" (issue detail description).
    onFocusChanged: ((Boolean) -> Unit)? = null,
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
    val currentOnFocusChanged by rememberUpdatedState(onFocusChanged)

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

    // Report focus transitions to the host (same focusedRowId idiom the toolbar
    // registration below uses). load() never fires onEdit, so a live remote-apply
    // driven off this signal doesn't loop back through onChange.
    LaunchedEffect(model.focusedRowId) {
        currentOnFocusChanged?.invoke(model.focusedRowId != null)
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
            if (bytes == null) {
                // No preview bytes — fall back to upload-then-insert (nothing to
                // show while the upload runs).
                val url = runCatching { uploader(uri) }.getOrNull() ?: return@launch
                model.insertImageUrl(url, alt = "image")
                return@launch
            }
            // Insert the block immediately (local preview), then run the host
            // upload through the model so the tile shows an uploading overlay
            // and, on failure, a Retry/remove affordance (iOS editor parity).
            // The host uploader returns either a real /api/attachments/... URL
            // (eager upload) or a draft:// placeholder (deferred upload at
            // create time); either way the row's URL is swapped on success.
            val pending = PendingImage(uri, bytes, name, mime, size.width, size.height)
            val rowId = model.insertImageUrl(draftUrl(), alt = "image", pending = pending)
            model.runUpload(rowId) { uploader(uri) }
        }
    }

    // The formatting toolbar is rendered by a screen-level overlay so it can
    // float above the keyboard (see ProvideMarkdownToolbar). Register this
    // editor as the active one while one of its fields is focused, and hand the
    // overlay this editor's image-picker action (the launcher must stay in this
    // composition). Last-focus-wins; the identity guard avoids clobbering a
    // sibling editor that grabbed focus first.
    val toolbarController = LocalMarkdownToolbarController.current
    val imageEnabledFlag = imageUploadEnabled && onUploadImage != null
    if (toolbarController != null) {
        LaunchedEffect(model.focusedRowId) {
            if (model.focusedRowId != null) {
                toolbarController.activeModel = model
                toolbarController.onPickImage = {
                    pickImage.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                }
                toolbarController.imageEnabled = imageEnabledFlag
            } else if (toolbarController.activeModel === model) {
                toolbarController.activeModel = null
            }
        }
        DisposableEffect(Unit) {
            onDispose { if (toolbarController.activeModel === model) toolbarController.activeModel = null }
        }
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = minHeight),
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

/** A team member offered by @mention autocomplete. */
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
