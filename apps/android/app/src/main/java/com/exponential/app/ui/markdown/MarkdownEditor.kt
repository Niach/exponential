package com.exponential.app.ui.markdown

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import android.content.Context
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.ui.markdown.model.PendingImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Block-based markdown editor / viewer. In `editable` mode it renders the
 * [EditorModel]'s rows as one multi-line field per text run between images
 * (EXP-534 — selection spans paragraphs) plus a formatting toolbar; in
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
    // Whether the floating formatting toolbar registers for this editor. The
    // comment composer opts out (EXP-246) — it carries its own image/@/# row.
    showToolbar: Boolean = true,
    // Reports whether any field of this editor holds focus. Lets the host gate a
    // live remote-apply on "not currently editing" (issue detail description).
    onFocusChanged: ((Boolean) -> Unit)? = null,
    // Optional hoisted model so a host can drive focus / caret insertion (the
    // issue-detail comment composer, EXP-240). Null keeps the private default.
    model: EditorModel? = null,
    // EXP-327: non-null adds a "Files" entry to the toolbar's image button and
    // receives the NON-image picks (images are inlined into the description
    // here instead — the host never sees them). Null keeps the plain image
    // button, for editors whose host has nowhere to put an attachment.
    onAttachFile: ((Uri) -> Unit)? = null,
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
    @Suppress("NAME_SHADOWING")
    val model = model ?: remember { EditorModel() }
    val currentOnChange by rememberUpdatedState(onChange)
    val currentInitialPending by rememberUpdatedState(initialPendingImages)
    val currentOnFocusChanged by rememberUpdatedState(onFocusChanged)

    // Wire edits → markdown out (once; closure reads the latest onChange).
    LaunchedEffect(model) {
        model.onEdit = { currentOnChange(model.currentMarkdown()) }
    }

    // Load external markdown only — never an echo of the user's own keystrokes,
    // including a STALE one the model has already moved past (EXP-655; see
    // EditorModel.reconcileHostMarkdown).
    LaunchedEffect(markdown) {
        if (model.reconcileHostMarkdown(markdown)) {
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

    val pickImage = rememberMarkdownImagePicker(model, onUploadImage)
    val pickFile = rememberMarkdownFilePicker(model, onUploadImage, onAttachFile)

    // The formatting toolbar is rendered by a screen-level overlay so it can
    // float above the keyboard (see ProvideMarkdownToolbar). Register this
    // editor as the active one while one of its fields is focused, and hand the
    // overlay this editor's image-picker action (the launcher must stay in this
    // composition). Last-focus-wins; the identity guard avoids clobbering a
    // sibling editor that grabbed focus first.
    val toolbarController = LocalMarkdownToolbarController.current
    val imageEnabledFlag = imageUploadEnabled && onUploadImage != null
    if (toolbarController != null && showToolbar) {
        LaunchedEffect(model.focusedRowId) {
            if (model.focusedRowId != null) {
                toolbarController.activeModel = model
                toolbarController.onPickImage = pickImage
                toolbarController.onPickFile = pickFile
                toolbarController.imageEnabled = imageEnabledFlag
            } else if (toolbarController.activeModel === model) {
                toolbarController.activeModel = null
            }
        }
        DisposableEffect(Unit) {
            onDispose { if (toolbarController.activeModel === model) toolbarController.activeModel = null }
        }
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = minHeight),
    ) {
        // The empty band below (and beside) the rows — the fields are
        // content-height, so a tap there used to fall through to the
        // screen's tap-outside catcher and dismiss the keyboard. Behind the
        // rows, it only receives what no field claims, and puts the caret at
        // the very end of the document (EXP-655). Consuming the tap keeps
        // the catcher out of it.
        Box(
            Modifier
                .matchParentSize()
                .pointerInput(model) {
                    detectTapGestures(onTap = { model.focusEnd() })
                },
        )
        Column(modifier = Modifier.fillMaxWidth()) {
            val rows = model.rows
            val soleEmptyId = rows.singleOrNull()?.let { (it as? EditorRow.TextRun)?.takeIf { p -> p.text.isEmpty() }?.id }
            rows.forEach { row ->
                key(row.id) {
                    when (row) {
                        is EditorRow.TextRun -> BlockTextField(
                            model = model,
                            row = row,
                            placeholder = if (row.id == soleEmptyId) placeholder else null,
                            mentionMembers = mentionMembers,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        is EditorRow.Image -> BlockImageEditView(model = model, row = row)
                        is EditorRow.Table -> TableRowEditView(model = model, row = row)
                    }
                }
            }
        }
    }
}

/**
 * A system photo-picker launcher wired to [model]'s image-block insert/upload
 * flow, returned as a plain launch function. [MarkdownEditor] registers it on
 * the shared toolbar controller for the floating toolbar's Image button; hosts
 * with their own pick-image affordance (the issue-detail comment composer,
 * EXP-240) call this directly so the picked image always lands in THEIR model —
 * the toolbar controller's last-focus-wins slot may still point at another
 * editor (e.g. the description) and is never safe to borrow.
 */
@Composable
fun rememberMarkdownImagePicker(
    model: EditorModel,
    onUploadImage: (suspend (uri: Uri) -> String?)?,
): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val currentModel by rememberUpdatedState(model)
    val currentUploader by rememberUpdatedState(onUploadImage)
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia(),
    ) { uri: Uri? ->
        val target = currentModel
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
                target.insertImageUrl(url, alt = "image")
                return@launch
            }
            // Insert the block immediately (local preview), then run the host
            // upload through the model so the tile shows an uploading overlay
            // and, on failure, a Retry/remove affordance (iOS editor parity).
            // The host uploader returns either a real /api/attachments/... URL
            // (eager upload) or a draft:// placeholder (deferred upload at
            // create time); either way the row's URL is swapped on success.
            val pending = PendingImage(uri, bytes, name, mime, size.width, size.height)
            val rowId = target.insertImageUrl(draftUrl(), alt = "image", pending = pending)
            target.runUpload(rowId) { uploader(uri) }
        }
    }
    return remember(launcher) {
        { launcher.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
    }
}

/**
 * The "Files" half of the toolbar's attach menu (EXP-327): an any-type document
 * picker that sorts the pick itself instead of making the user pick the right
 * button up front.
 *
 * An inline-image pick is APPENDED to the description — same insert/upload
 * lifecycle as the photo picker, just at the end rather than at the caret,
 * because the user was attaching rather than typing. Anything else goes to
 * [onAttachFile] and becomes a real attachment row. This is why picking an
 * image here no longer dead-ends in "images go in the description": it just
 * goes there.
 *
 * Returns null when there is nowhere to put a non-image file, which is also the
 * signal that the toolbar should keep its plain image button.
 */
@Composable
fun rememberMarkdownFilePicker(
    model: EditorModel,
    onUploadImage: (suspend (uri: Uri) -> String?)?,
    onAttachFile: ((Uri) -> Unit)?,
): (() -> Unit)? {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val currentModel by rememberUpdatedState(model)
    val currentUploader by rememberUpdatedState(onUploadImage)
    val currentAttach by rememberUpdatedState(onAttachFile)
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        val target = currentModel
        val uploader = currentUploader
        val attach = currentAttach
        scope.launch {
            // Fall back to octet-stream, NOT the photo picker's image/jpeg: an
            // untyped document is a file, not a picture.
            val mime = canonicalContentType(
                withContext(Dispatchers.IO) {
                    MarkdownMediaUtils.guessMimeType(context, uri, fallback = "application/octet-stream")
                }
            )
            if (!isInlineImage(mime)) {
                attach?.invoke(uri)
                return@launch
            }
            if (uploader == null) return@launch
            appendPickedImage(context, target, uri, mime, uploader)
        }
    }
    return remember(launcher, onAttachFile) {
        if (onAttachFile == null) null else ({ launcher.launch(arrayOf("*/*")) })
    }
}

/**
 * Append an already-classified inline image to [model]'s description and run
 * the host [uploader] against the inserted row (EXP-327).
 *
 * Shared by the toolbar's "Files" pick and the issue-detail fallback for an
 * image that reached the attachment path anyway, so both produce the identical
 * end-of-description block with the same preview/retry lifecycle as the photo
 * picker.
 */
suspend fun appendPickedImage(
    context: Context,
    model: EditorModel,
    uri: Uri,
    contentType: String,
    uploader: suspend (Uri) -> String?,
) {
    // ALL ContentResolver work rides Dispatchers.IO (same rule as
    // IssueDetailViewModel.runUpload): callers launch this on the composition's
    // Main scope, and an OpenDocument pick from a cloud-backed DocumentsProvider
    // streams the bytes over the network inside openInputStream/readBytes.
    // Snapshot-state writes stay on the caller's dispatcher.
    val pending = withContext(Dispatchers.IO) {
        val bytes = MarkdownMediaUtils.readBytes(context, uri) ?: return@withContext null
        val name = MarkdownMediaUtils.guessFilename(context, uri)
        val size = MarkdownMediaUtils.probeSize(context, uri)
        PendingImage(uri, bytes, name, contentType, size.width, size.height)
    }
    if (pending == null) {
        // No preview bytes — upload first, then insert (nothing to show while
        // the upload runs).
        val url = runCatching { uploader(uri) }.getOrNull() ?: return
        model.appendImageUrl(url, alt = "image")
        return
    }
    val rowId = model.appendImageUrl(draftUrl(), alt = "image", pending = pending)
    model.runUpload(rowId) { uploader(uri) }
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
