package com.exponential.app.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import com.exponential.app.domain.ImageActions
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.fileProviderUri
import com.exponential.app.ui.issue.shareFile
import com.exponential.app.ui.work.loadTallImageBytes
import com.exponential.app.ui.work.tallImageEntryPoint
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// #EXP-1926 / #EXP-1948 (fixtures/image-actions.json): a content picture's
// long-press menu, Copy image then Share, over THE GlassDropdownMenu. The
// bytes are the ORIGINAL picture's, through the existing authenticated path
// (the Tall cache / AttachmentsApi.download; genUI's host hook), as PNG.

/** Where a picture's original bytes come from (thrown = the failure toast). */
fun interface ImageBytes {
    suspend fun load(): ByteArray
}

/** The original bytes of a Markdown / result src: an attachment through the Tall cache, a `data:` image decoded, else a download as the active account. */
fun imageBytesForSrc(context: Context, src: String): ImageBytes = ImageBytes {
    val auth = tallImageEntryPoint(context).authRepository()
    val origin = auth.instanceUrl.value.orEmpty()
    val id = ImageActions.attachmentId(src, origin)
    val s = src.trim()
    when {
        id != null -> loadTallImageBytes(context, id)
        s.startsWith("data:", ignoreCase = true) -> dataUriBytes(s)
        else -> {
            val account = auth.activeAccountId.value ?: error("signed out")
            tallImageEntryPoint(context).attachmentsApi().download(account, s)
        }
    }
}

/** An attachment's original bytes (the Tall cache: memory → disk → network). */
fun attachmentImageBytes(context: Context, attachmentId: String): ImageBytes = ImageBytes { loadTallImageBytes(context, attachmentId) }

private fun dataUriBytes(src: String): ByteArray {
    val comma = src.indexOf(',')
    require(comma > 0) { "data uri without a comma" }
    val meta = src.substring(5, comma)
    val payload = src.substring(comma + 1)
    return if (meta.endsWith(";base64", ignoreCase = true)) {
        java.util.Base64.getMimeDecoder().decode(payload)
    } else {
        java.net.URLDecoder.decode(payload.replace("+", "%2B"), "UTF-8").toByteArray(Charsets.ISO_8859_1)
    }
}

/** The PNG of [bytes] (`copyEncoding`): PNG as it is, anything else decoded (a GIF's first frame) and re-encoded, never downscaled. */
fun pngOf(bytes: ByteArray): ByteArray {
    if (ImageActions.copyEncoding(bytes) == ImageActions.Encoding.Passthrough) return bytes
    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: error("undecodable picture")
    return try {
        ByteArrayOutputStream().also { out ->
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) { "png encode failed" }
        }.toByteArray()
    } finally {
        bitmap.recycle()
    }
}

/** The PNG under the FileProvider's `image-actions/` path, as [name]. */
private fun writeImageFile(context: Context, png: ByteArray, name: String): File {
    val root = File(context.cacheDir, "image-actions")
    // A pasted or shared picture stays readable for a day; older ones go.
    val stale = System.currentTimeMillis() - 24 * 60 * 60 * 1000L
    root.listFiles()?.filter { it.lastModified() < stale }?.forEach { it.deleteRecursively() }
    val dir = File(root, UUID.randomUUID().toString()).apply { mkdirs() }
    return File(dir, name).apply { writeBytes(png) }
}

/** The bytes as a PNG file under the FileProvider (IO). */
private suspend fun pngFile(context: Context, bytes: ImageBytes, name: String?): File {
    val raw = bytes.load()
    return withContext(Dispatchers.IO) { writeImageFile(context, pngOf(raw), ImageActions.shareFileName(name)) }
}

/** Copy image: the PNG's `content://` on the clipboard; Share: ACTION_SEND with it. Toasts per the fixture. */
class ImageActionsRunner(private val context: Context, private val toaster: Toaster?, private val scope: kotlinx.coroutines.CoroutineScope) {
    fun copy(bytes: ImageBytes, name: String?) {
        scope.launch {
            try {
                val file = pngFile(context, bytes, name)
                val uri = fileProviderUri(context, file)
                val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                cm.setPrimaryClip(ClipData.newUri(context.contentResolver, ImageActions.Copy.COPY_IMAGE, uri))
                toaster?.success(ImageActions.Copy.COPIED)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                toaster?.error(ImageActions.Copy.COPY_FAILED)
            }
        }
    }

    fun share(bytes: ImageBytes, name: String?) {
        scope.launch {
            try {
                val file = pngFile(context, bytes, name)
                shareFile(context, file, "image/png")
            } catch (e: CancellationException) {
                throw e
            } catch (_: Throwable) {
                toaster?.error(ImageActions.Copy.SHARE_FAILED)
            }
        }
    }
}

@Composable
fun rememberImageActions(): ImageActionsRunner {
    val context = LocalContext.current
    val toaster = LocalOverlayToaster.current
    val scope = rememberCoroutineScope()
    return remember(context, toaster, scope) { ImageActionsRunner(context, toaster, scope) }
}

/** The image menu (`imageMenu(touch, true)`): Copy image, then Share. */
@Composable
fun ImageActionsMenu(expanded: Boolean, onDismiss: () -> Unit, bytes: ImageBytes, name: String? = null) {
    val actions = rememberImageActions()
    GlassDropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        for (item in ImageActions.imageMenu(ImageActions.Input.Touch, canShareFile = true)) {
            when (item) {
                ImageActions.Item.CopyImage -> GlassMenuItem(
                    text = { Text(ImageActions.Copy.COPY_IMAGE) },
                    leadingIcon = { Icon(ExpIcons.uiCopy, contentDescription = null) },
                    onClick = {
                        onDismiss()
                        actions.copy(bytes, name)
                    },
                    modifier = Modifier.testTag(ImageActions.TestIds.COPY_IMAGE),
                )
                ImageActions.Item.Share -> GlassMenuItem(
                    text = { Text(ImageActions.Copy.SHARE) },
                    leadingIcon = { Icon(ExpIcons.uiShare, contentDescription = null) },
                    onClick = {
                        onDismiss()
                        actions.share(bytes, name)
                    },
                    modifier = Modifier.testTag(ImageActions.TestIds.SHARE),
                )
            }
        }
    }
}

/**
 * A content picture that answers a long press with [ImageActionsMenu]
 * (`pressMenu`): only once its bytes rendered ([bytes] non-null), else the
 * press falls through to the element around it. A tap runs [onClick] as
 * before (null = nothing); a drag still scrolls.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ImageActionsBox(
    bytes: ImageBytes?,
    modifier: Modifier = Modifier,
    name: String? = null,
    onClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val gesture = when {
        bytes != null -> Modifier.combinedClickable(
            role = Role.Image,
            onClick = { onClick?.invoke() },
            onLongClick = { open = true },
        )
        onClick != null -> Modifier.combinedClickable(role = Role.Image, onClick = onClick)
        else -> Modifier
    }
    Box(modifier.then(gesture)) {
        content()
        if (bytes != null) {
            Box(Modifier.align(Alignment.Center)) {
                ImageActionsMenu(expanded = open, onDismiss = { open = false }, bytes = bytes, name = name)
            }
        }
    }
}

/**
 * A long press that takes nothing else (a zoom's pinch, pan and double tap,
 * a pager's swipe): the down stays unconsumed; a move past the touch slop,
 * a second finger or a consumed change cancels it; once it fires the rest
 * of the gesture is consumed (no tap on release).
 */
fun Modifier.imageLongPress(enabled: Boolean, onLongPress: () -> Unit): Modifier = if (!enabled) this else pointerInput(onLongPress) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        var cancelled = false
        val ended = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
            var done = false
            while (!done) {
                val e = awaitPointerEvent()
                val c = e.changes.firstOrNull { it.id == down.id } ?: e.changes.first()
                if (!c.pressed) {
                    done = true
                } else if (e.changes.size > 1 || c.isConsumed || (c.position - down.position).getDistance() > viewConfiguration.touchSlop) {
                    cancelled = true
                    done = true
                }
            }
            true
        }
        if (ended == null && !cancelled) {
            onLongPress()
            while (true) {
                val e = awaitPointerEvent()
                e.changes.forEach { it.consume() }
                if (e.changes.none { it.pressed }) break
            }
        }
    }
}

/** The active instance's origin (THE `imageSource` origin); empty signed out. */
@Composable
fun rememberInstanceOrigin(): String {
    val context = LocalContext.current
    val auth = remember(context) { tallImageEntryPoint(context).authRepository() }
    val origin by auth.instanceUrl.collectAsState()
    return origin.orEmpty().trimEnd('/')
}
