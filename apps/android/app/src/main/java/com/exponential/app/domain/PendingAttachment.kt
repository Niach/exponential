package com.exponential.app.domain

import android.net.Uri

/**
 * A file the user picked for the next send (EXP-554), generalizing the steer
 * composer's pending image (EXP-511) to any content type.
 *
 * [uploadedId] is stamped once the upload succeeded, so retrying after a
 * mid-batch failure never re-uploads what already landed.
 *
 * Compose-free on purpose (EXP-621): the steer connection in `data/steer`
 * holds these across screens, so the type cannot live next to the strip that
 * draws them.
 */
data class PendingAttachment(
    val uri: Uri,
    val bytes: ByteArray,
    val filename: String,
    /** Already canonicalized (`canonicalContentType`) by whoever built this. */
    val contentType: String,
    /** True for the five inline-embeddable raster types — the ones that upload
     *  to `/images` and render as a thumbnail. */
    val isImage: Boolean,
    val uploadedId: String? = null,
) {
    // ByteArray breaks data-class equality; compare by the scalar fields only.
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is PendingAttachment) return false
        return uri == other.uri && filename == other.filename &&
            contentType == other.contentType && isImage == other.isImage &&
            uploadedId == other.uploadedId
    }

    override fun hashCode(): Int {
        var result = uri.hashCode()
        result = 31 * result + filename.hashCode()
        result = 31 * result + contentType.hashCode()
        result = 31 * result + isImage.hashCode()
        result = 31 * result + (uploadedId?.hashCode() ?: 0)
        return result
    }
}
