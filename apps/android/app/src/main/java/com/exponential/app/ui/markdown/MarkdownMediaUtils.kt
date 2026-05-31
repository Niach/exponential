package com.exponential.app.ui.markdown

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap

/** Probed intrinsic pixel size of an image, used to pre-size the editor tile. */
data class ProbedSize(val width: Int?, val height: Int?)

/**
 * Picked-image helpers for the editor's image pipeline: MIME / filename
 * resolution (relocated from the old toolbar) plus bounds-only dimension probing
 * so we can reserve aspect-ratio space before upload (parity with iOS
 * `pixelSize(of:)`).
 */
object MarkdownMediaUtils {

    fun guessMimeType(context: Context, uri: Uri, fallback: String = "image/jpeg"): String {
        val resolver = context.contentResolver
        resolver.getType(uri)?.takeIf { it.isNotBlank() }?.let { return it }
        val ext = MimeTypeMap.getFileExtensionFromUrl(uri.toString())
        if (!ext.isNullOrBlank()) {
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)?.let { return it }
        }
        return fallback
    }

    fun guessFilename(context: Context, uri: Uri): String {
        val resolver = context.contentResolver
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (cursor.moveToFirst() && idx >= 0) {
                cursor.getString(idx)?.takeIf { it.isNotBlank() }?.let { return it }
            }
        }
        return uri.lastPathSegment ?: "image"
    }

    /** Decode just the image bounds to get pixel w/h without loading the bitmap. */
    fun probeSize(context: Context, uri: Uri): ProbedSize {
        return try {
            val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, opts) }
            ProbedSize(
                width = opts.outWidth.takeIf { it > 0 },
                height = opts.outHeight.takeIf { it > 0 },
            )
        } catch (_: Throwable) {
            ProbedSize(null, null)
        }
    }

    fun readBytes(context: Context, uri: Uri): ByteArray? =
        try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        }
}
