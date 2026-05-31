package com.exponential.app.data.share

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.IntentCompat
import androidx.core.net.toUri
import java.io.File
import java.util.UUID

/**
 * Parsed result of an `ACTION_SEND` / `ACTION_SEND_MULTIPLE` intent. Image URIs
 * here are stable `file://` URIs inside the app cache — the original
 * `content://` URIs are copied at parse time (see [ShareIntentParser.parse]).
 */
data class SharedPayload(
    val text: String?,
    val subject: String?,
    val imageUris: List<Uri>,
)

/**
 * Turns a system share intent into a [SharedPayload].
 *
 * The single most important thing this does is **copy shared images into the
 * app cache synchronously, while the intent's temporary read grant is still
 * live**. That grant is scoped to the receiving task and is revoked once the
 * task finishes; by the time the user has picked a project and tapped Post the
 * original `content://` URI may no longer be readable. Copying up front to
 * `cacheDir/share-inbox` and handing downstream code stable `file://` URIs
 * removes that whole failure class — `ContentResolver.openInputStream` (used by
 * the existing image-upload path) works on `file://` just the same.
 */
object ShareIntentParser {

    private const val INBOX_DIR = "share-inbox"
    private const val MAX_IMAGES = 10
    private val STALE_AGE_MS = 24L * 60 * 60 * 1000 // prune cache entries older than a day

    fun isShareIntent(intent: Intent?): Boolean =
        intent?.action == Intent.ACTION_SEND || intent?.action == Intent.ACTION_SEND_MULTIPLE

    fun parse(context: Context, intent: Intent): SharedPayload? {
        if (!isShareIntent(intent)) return null
        pruneStaleInbox(context)

        val text = intent.getStringExtra(Intent.EXTRA_TEXT)?.takeIf { it.isNotBlank() }
        val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT)?.takeIf { it.isNotBlank() }

        // IntentCompat picks the typed getParcelableExtra(name, Class) on API 33+
        // and the deprecated overload below it — no manual SDK_INT branch needed.
        val rawUris: List<Uri> = when (intent.action) {
            Intent.ACTION_SEND ->
                listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE ->
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
            else -> emptyList()
        }

        // Some senders populate only ClipData, not EXTRA_STREAM.
        val clipUris = intent.clipData?.let { clip ->
            (0 until clip.itemCount).mapNotNull { clip.getItemAt(it).uri }
        }.orEmpty()

        val imageUris = (rawUris + clipUris)
            .distinct()
            .filter { isImage(context, it) }
            .take(MAX_IMAGES)
            .mapNotNull { copyToCache(context, it) }

        if (text == null && imageUris.isEmpty()) return null
        return SharedPayload(text = text, subject = subject, imageUris = imageUris)
    }

    private fun isImage(context: Context, uri: Uri): Boolean =
        context.contentResolver.getType(uri)?.startsWith("image/") == true

    /** Copy [src] into `cacheDir/share-inbox`, returning a stable `file://` Uri. */
    private fun copyToCache(context: Context, src: Uri): Uri? = runCatching {
        val dir = File(context.cacheDir, INBOX_DIR).apply { mkdirs() }
        // Preserve the extension so downstream filename/MIME inference still works.
        val ext = context.contentResolver.getType(src)
            ?.substringAfterLast('/', "")
            ?.takeIf { it.isNotBlank() }
            ?: "img"
        val dest = File(dir, "${UUID.randomUUID()}.$ext")
        val copied = context.contentResolver.openInputStream(src)?.use { input ->
            dest.outputStream().use { input.copyTo(it) }
            true
        } ?: false
        if (copied) dest.toUri() else null
    }.getOrNull()

    private fun pruneStaleInbox(context: Context) {
        runCatching {
            val dir = File(context.cacheDir, INBOX_DIR)
            if (!dir.isDirectory) return
            val cutoff = System.currentTimeMillis() - STALE_AGE_MS
            dir.listFiles()?.forEach { file ->
                if (file.lastModified() < cutoff) file.delete()
            }
        }
    }
}
