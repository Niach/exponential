package at.exponential.ui.compose

import android.content.ClipboardManager
import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalTextToolbar
import androidx.compose.ui.platform.TextToolbar
import androidx.compose.ui.platform.TextToolbarStatus
import at.exponential.ui.ExponentialUi
import at.exponential.ui.host.PickedFile

/**
 * #EDE-65 R (interactions-round1.json composerPaste): the files on the
 * system clipboard, in item order, with their content URIs: each clip
 * item's URI, named by the provider (`OpenableColumns`), else
 * `image.<ext of its type>` (`image.png`) for a raw image. Text items are
 * no files (the field's own paste). `SurfaceModel.composerPaste` decides.
 */
internal fun composerClipboard(context: Context? = ExponentialUi.appContext): Pair<List<PickedFile>, List<String>> {
    val ctx = context ?: return emptyList<PickedFile>() to emptyList()
    val clip = runCatching { (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip }.getOrNull() ?: return emptyList<PickedFile>() to emptyList()
    val files = ArrayList<PickedFile>()
    val uris = ArrayList<String>()
    for (i in 0 until clip.itemCount) {
        val uri: Uri = clip.getItemAt(i).uri ?: continue
        val type = ctx.contentResolver.getType(uri) ?: "application/octet-stream"
        var name: String? = null
        var size = 0L
        runCatching {
            ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    name = c.getString(0)
                    size = if (c.isNull(1)) 0L else c.getLong(1)
                }
            }
        }
        val ext = MimeTypeMap.getSingleton().getExtensionFromMimeType(type) ?: "png"
        files.add(PickedFile(name?.takeIf { it.isNotBlank() } ?: (if (type.startsWith("image/")) "image.$ext" else "file.$ext"), size, type))
        uris.add(uri.toString())
    }
    return files to uris
}

/**
 * The selection toolbar's Paste goes through [onPaste] first (`true` =
 * taken, the field's own paste skipped) while [enabled]: a Composer's
 * image paste. The platform offers Paste only while the clipboard holds
 * text; a hardware Ctrl/⌘+V (`OwnedTextField`) reaches an image-only one.
 */
@Composable
internal fun ComposerPasteToolbar(enabled: Boolean, onPaste: () -> Boolean, content: @Composable () -> Unit) {
    if (!enabled) return content()
    val base = LocalTextToolbar.current
    val toolbar = remember(base, onPaste) {
        object : TextToolbar {
            override val status: TextToolbarStatus get() = base.status
            override fun hide() = base.hide()
            override fun showMenu(rect: Rect, onCopyRequested: (() -> Unit)?, onPasteRequested: (() -> Unit)?, onCutRequested: (() -> Unit)?, onSelectAllRequested: (() -> Unit)?) {
                val paste = onPasteRequested?.let { own -> { if (!onPaste()) own() } }
                base.showMenu(rect, onCopyRequested, paste, onCutRequested, onSelectAllRequested)
            }
        }
    }
    CompositionLocalProvider(LocalTextToolbar provides toolbar, content = content)
}
