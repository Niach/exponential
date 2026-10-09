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
    /** Wave D: the server's sanitized `filename` from the upload response —
     *  a steer file line's link text. Stamped together with [uploadedId]. */
    val uploadedName: String? = null,
    // EXP-824: a `video/*` / `audio/*` pick, already normalised by the
    // MediaPreparer (bytes = the 720p H.264/AAC MP4 for video). The poster
    // JPEG and probed metadata ride the same multipart upload as extra parts.
    val isMedia: Boolean = false,
    val poster: ByteArray? = null,
    val width: Int? = null,
    val height: Int? = null,
    val durationMs: Long? = null,
) {
    // ByteArray breaks data-class equality; compare by the scalar fields only.
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is PendingAttachment) return false
        return uri == other.uri && filename == other.filename &&
            contentType == other.contentType && isImage == other.isImage &&
            uploadedId == other.uploadedId && uploadedName == other.uploadedName && isMedia == other.isMedia &&
            width == other.width && height == other.height && durationMs == other.durationMs
    }

    override fun hashCode(): Int {
        var result = uri.hashCode()
        result = 31 * result + filename.hashCode()
        result = 31 * result + contentType.hashCode()
        result = 31 * result + isImage.hashCode()
        result = 31 * result + (uploadedId?.hashCode() ?: 0)
        result = 31 * result + (uploadedName?.hashCode() ?: 0)
        result = 31 * result + isMedia.hashCode()
        result = 31 * result + (durationMs?.hashCode() ?: 0)
        return result
    }
}

/** Wave D: the uploaded image ids, in pick order (image #k = the k-th). */
fun List<PendingAttachment>.uploadedImageIds(): List<String> =
    filter { it.isImage }.mapNotNull { it.uploadedId }

/** Wave D: the uploaded non-image files as steer file lines, in pick order. */
fun List<PendingAttachment>.uploadedSteerFiles(): List<SteerFile> =
    filter { !it.isImage }.mapNotNull { a -> a.uploadedId?.let { SteerFile(it, a.uploadedName ?: a.filename) } }

/** Wave D: the 1-based `[Image #k]` number of the image at [index], or null
 *  when the entry there is a file (files carry no marker). */
fun List<PendingAttachment>.imageNumberAt(index: Int): Int? = steerImageNumberAt(map { it.isImage }, index)

/** [imageNumberAt] over the pending set's kinds (true = image), in pick order. */
fun steerImageNumberAt(isImage: List<Boolean>, index: Int): Int? {
    if (index !in isImage.indices || !isImage[index]) return null
    return isImage.take(index + 1).count { it }
}
