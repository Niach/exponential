package com.exponential.app.ui.agent

import android.content.Context
import android.net.Uri
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.domain.STEER_ATTACHMENT_REJECTED
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Wave D: one "Add file or image" pick (ANY type) read for the Agent page and
 * steer composers. A provider that advertises more than the file cap is
 * refused BEFORE its bytes are read (a 2 GB video would not fit in memory);
 * the composers apply the per-kind caps ([steerAttachmentRefusal]) after.
 */
sealed interface ComposerPick {
    data class Read(val uri: Uri, val bytes: ByteArray, val filename: String, val mime: String) : ComposerPick
    data class Refused(val message: String) : ComposerPick
}

suspend fun readComposerPick(context: Context, uri: Uri): ComposerPick = withContext(Dispatchers.IO) {
    val advertised = MarkdownMediaUtils.querySize(context, uri)
    if (advertised != null && advertised > MAX_FILE_UPLOAD_BYTES) {
        return@withContext ComposerPick.Refused(STEER_ATTACHMENT_REJECTED)
    }
    val bytes = MarkdownMediaUtils.readBytes(context, uri)
        ?: return@withContext ComposerPick.Refused("That file could not be read")
    ComposerPick.Read(
        uri = uri,
        bytes = bytes,
        filename = MarkdownMediaUtils.guessFilename(context, uri),
        mime = MarkdownMediaUtils.guessMimeType(context, uri, fallback = "application/octet-stream"),
    )
}

/**
 * #EXP-1841: one batch of picks (a picker result, a share) as the composer's
 * pending entries, in pick order, and the messages of those refused before
 * they were read ([stagePendingAttachments] stages both at once).
 */
data class ComposerPickBatch(
    val attachments: List<com.exponential.app.domain.PendingAttachment>,
    val refusals: List<String>,
)

fun List<ComposerPick>.toPickBatch(): ComposerPickBatch = ComposerPickBatch(
    attachments = filterIsInstance<ComposerPick.Read>().map { pick ->
        val contentType = com.exponential.app.domain.canonicalContentType(pick.mime)
        com.exponential.app.domain.PendingAttachment(
            uri = pick.uri,
            bytes = pick.bytes,
            filename = pick.filename,
            contentType = contentType,
            isImage = com.exponential.app.domain.isInlineImage(contentType),
        )
    },
    refusals = filterIsInstance<ComposerPick.Refused>().map { it.message },
)
