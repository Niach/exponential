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
    /** EXP-1537: the client's own key for this pick, kept across [copy]
     *  (an upload stamp): a settled send removes exactly the picks it took
     *  ([ComposerSend.settle]), whatever was added or removed meanwhile. */
    val key: String = java.util.UUID.randomUUID().toString(),
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

/** #EXP-1841: the strip's kinds, in order (the fixture's `strip`). */
fun List<PendingAttachment>.attachmentKinds(): List<ComposerAttachments.Kind> =
    map { if (it.isImage) ComposerAttachments.Kind.Image else ComposerAttachments.Kind.File }

/** #EXP-1841: a composer after a pick ([stagePendingAttachments]). */
data class PendingStage(
    val attachments: List<PendingAttachment>,
    val text: String,
    val caret: Int,
    /** The error toast (the stage's toasts in order, one per line), or null. */
    val error: String?,
)

/**
 * #EXP-1841 (composer-attachments.json): THE attach of every composer. [picks]
 * = the files read, in pick order; [refusals] = the picks refused before their
 * bytes were read (the rejection copy counts as a `rejected` pick, any other
 * message, like an unreadable file, is appended to the toast). Each TAKEN
 * image drops its `[Image #N]` at [caret] ([ComposerAttachments.stage]).
 */
fun stagePendingAttachments(
    current: List<PendingAttachment>,
    picks: List<PendingAttachment>,
    refusals: List<String>,
    text: String,
    caret: Int,
): PendingStage {
    val kinds = picks.map { ComposerAttachments.classify(it.isImage, it.bytes.size.toLong()) } +
        refusals.filter { it == ComposerAttachments.REJECTED_TOAST }.map { ComposerAttachments.PickKind.Rejected }
    val staged = ComposerAttachments.stage(current.attachmentKinds(), kinds, text, caret)
    val taken = picks.filterIndexed { index, _ -> staged.taken[index] }
    val toasts = (staged.toasts + refusals.filter { it != ComposerAttachments.REJECTED_TOAST }).distinct()
    return PendingStage(
        attachments = current + taken,
        text = staged.text,
        caret = staged.caret,
        error = toasts.joinToString("\n").ifEmpty { null },
    )
}

/** #EXP-1841: the strip's X ([ComposerAttachments.drop]): the list without
 *  the entry at [index] and the draft without its image's markers. */
fun dropPendingAttachment(
    current: List<PendingAttachment>,
    index: Int,
    text: String,
): Pair<List<PendingAttachment>, String> {
    if (index !in current.indices) return current to text
    val dropped = ComposerAttachments.drop(current.attachmentKinds(), index, text)
    return current.filterIndexed { i, _ -> i != index } to dropped.text
}
