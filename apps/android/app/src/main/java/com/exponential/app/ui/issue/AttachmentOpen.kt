package com.exponential.app.ui.issue

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.FileProvider
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.domain.sanitizeFilename
import java.io.File

/**
 * Handing a downloaded attachment to another app — shared by the issue's Files
 * section (EXP-297) and the comment attachment strips (EXP-554), which open
 * their tiles the same way (there is no in-app image viewer).
 */
internal fun fileProviderUri(context: Context, file: File): Uri =
    FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)

/**
 * Hand the cached bytes to whatever app can render them. The chooser carries
 * the read grant; if the device has nothing at all for the type we fall back
 * to a share sheet, which at least lets the user save it somewhere.
 */
internal fun openFile(context: Context, file: File, contentType: String) {
    val uri = fileProviderUri(context, file)
    val view = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, contentType.ifBlank { "*/*" })
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    val chooser = Intent.createChooser(view, "Open with")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    try {
        context.startActivity(chooser)
    } catch (_: ActivityNotFoundException) {
        shareFile(context, file, contentType)
    }
}

internal fun shareFile(context: Context, file: File, contentType: String) {
    val uri = fileProviderUri(context, file)
    val send = Intent(Intent.ACTION_SEND).apply {
        type = contentType.ifBlank { "*/*" }
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    val chooser = Intent.createChooser(send, "Share file")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    runCatching { context.startActivity(chooser) }
}

/**
 * Where an attachment's bytes are cached locally: one directory per attachment
 * id (attachments are immutable, so a file already at the expected size never
 * needs re-fetching) holding the sanitized server filename.
 */
internal fun attachmentCacheFile(context: Context, attachment: AttachmentEntity): File =
    File(
        File(File(context.cacheDir, "attachments"), attachment.id),
        sanitizeFilename(attachment.filename),
    )
