package com.exponential.app.data.share

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.core.content.IntentCompat
import androidx.core.net.toUri
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Parsed result of an `ACTION_SEND` / `ACTION_SEND_MULTIPLE` intent. Image URIs
 * here are stable `file://` URIs inside the app cache — the original
 * `content://` URIs are copied at parse time (see [ShareIntentParser.parse]).
 */
data class SharedPayload(
    val text: String?,
    val subject: String?,
    val imageUris: List<Uri>,
    /** EXP-1635: the non-image files, the chat path's file attachments. */
    val files: List<SharedFile> = emptyList(),
    /** A shared file over the upload cap: left unread, the composer refuses it. */
    val oversizeFiles: Boolean = false,
    /** EXP-1635: the sharing shortcut the share arrived on (`EXTRA_SHORTCUT_ID`). */
    val directShareId: String? = null,
)

/** A shared non-image file, cached under its own name (a cached Uri has no provider to ask). */
data class SharedFile(val uri: Uri, val filename: String, val mime: String)

/**
 * Turns a system share intent into a [SharedPayload].
 *
 * The single most important thing this does is **copy shared images into the
 * app cache immediately at parse time, while the intent's temporary read grant
 * is still live**. That grant is scoped to the receiving task and is revoked
 * once the task finishes; by the time the user has picked a board and tapped
 * Post the original `content://` URI may no longer be readable. Copying up
 * front to `cacheDir/share-inbox` and handing downstream code stable `file://`
 * URIs removes that whole failure class — `ContentResolver.openInputStream`
 * (used by the existing image-upload path) works on `file://` just the same.
 *
 * [parse] is suspend and rides every ContentResolver/disk touch on
 * Dispatchers.IO (REV-16): a cloud-backed provider (e.g. Google Photos with
 * "free up device storage") streams the ORIGINAL over the network inside
 * `openInputStream`, so up to [MAX_IMAGES] blocking full-file copies on the
 * main thread froze the launch frame into an ANR. Copying in a coroutine keeps
 * the grant window — it starts right at intent delivery and the receiving task
 * stays alive while it runs.
 */
object ShareIntentParser {

    private const val INBOX_DIR = "share-inbox"
    private const val MAX_IMAGES = 10
    private val STALE_AGE_MS = 24L * 60 * 60 * 1000 // prune cache entries older than a day

    fun isShareIntent(intent: Intent?): Boolean = ShareIntentPlan.isShareAction(intent?.action)

    suspend fun parse(context: Context, intent: Intent): SharedPayload? {
        // IntentCompat picks the typed getParcelableExtra(name, Class) on API 33+
        // and the deprecated overload below it — no manual SDK_INT branch needed.
        // Text rides getCharSequenceExtra: a sender's styled text is a
        // CharSequence, which getStringExtra answers with null.
        val plan = ShareIntentPlan.plan(
            ShareIntentPlan.Fields(
                action = intent.action,
                text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT),
                subject = intent.getCharSequenceExtra(Intent.EXTRA_SUBJECT),
                processText = intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT),
                streams = when (intent.action) {
                    Intent.ACTION_SEND ->
                        listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
                    Intent.ACTION_SEND_MULTIPLE ->
                        IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                            .orEmpty().filterNotNull()
                    else -> emptyList()
                },
                // Some senders populate only ClipData, not EXTRA_STREAM; the
                // system grants read on every ClipData Uri.
                clipUris = intent.clipData?.let { clip ->
                    (0 until clip.itemCount).mapNotNull { clip.getItemAt(it).uri }
                }.orEmpty(),
                shortcutId = intent.getStringExtra(Intent.EXTRA_SHORTCUT_ID),
            ),
        ) ?: return null

        // Everything past here talks to ContentResolver or the disk — even
        // getType() is a binder round-trip into the sending app's provider —
        // so it all rides IO (same rule as the markdown image picker).
        var oversizeFiles = false
        val imageUris = mutableListOf<Uri>()
        val files = mutableListOf<SharedFile>()
        withContext(Dispatchers.IO) {
            pruneStaleInbox(context)
            for (uri in plan.streams) {
                val displayName = runCatching { MarkdownMediaUtils.guessFilename(context, uri) }.getOrNull()
                val mime = ShareIntentPlan.mimeOf(
                    resolverType = runCatching { context.contentResolver.getType(uri) }.getOrNull(),
                    displayName = displayName,
                    intentType = intent.type,
                    mimeForExtension = { MimeTypeMap.getSingleton().getMimeTypeFromExtension(it) },
                )
                val image = ShareIntentPlan.isImage(mime)
                if (if (image) imageUris.size >= MAX_IMAGES else files.size >= MAX_IMAGES) continue
                val size = MarkdownMediaUtils.querySize(context, uri)
                if (!image && size != null && size > MAX_FILE_UPLOAD_BYTES) {
                    oversizeFiles = true
                    continue
                }
                val cached = copyToCache(context, uri, displayName, mime) ?: continue
                if (image) imageUris += cached.uri else files += cached
            }
        }

        if (plan.text == null && imageUris.isEmpty() && files.isEmpty() && !oversizeFiles) return null
        return SharedPayload(
            text = plan.text,
            subject = plan.subject,
            imageUris = imageUris,
            files = files,
            oversizeFiles = oversizeFiles,
            directShareId = plan.directShareId,
        )
    }

    /**
     * Copy [src] into its own UUID dir under `cacheDir/share-inbox`, named
     * after the sender's display name with an extension its type implies
     * ([ShareIntentPlan.cachedName]): the composer reads the file:// copy
     * and infers the same name and type from it.
     */
    private fun copyToCache(context: Context, src: Uri, displayName: String?, mime: String?): SharedFile? = runCatching {
        val filename = ShareIntentPlan.cachedName(displayName, mime) {
            MimeTypeMap.getSingleton().getExtensionFromMimeType(it)
        }
        val dir = File(File(context.cacheDir, INBOX_DIR), UUID.randomUUID().toString()).apply { mkdirs() }
        val dest = File(dir, filename)
        val copied = context.contentResolver.openInputStream(src)?.use { input ->
            dest.outputStream().use { input.copyTo(it) }
            true
        } ?: false
        if (copied) SharedFile(dest.toUri(), filename, mime ?: "application/octet-stream") else null
    }.getOrNull()

    private fun pruneStaleInbox(context: Context) {
        runCatching {
            val dir = File(context.cacheDir, INBOX_DIR)
            if (!dir.isDirectory) return
            val cutoff = System.currentTimeMillis() - STALE_AGE_MS
            dir.listFiles()?.forEach { file ->
                if (file.lastModified() < cutoff) file.deleteRecursively()
            }
        }
    }
}
