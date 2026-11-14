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
