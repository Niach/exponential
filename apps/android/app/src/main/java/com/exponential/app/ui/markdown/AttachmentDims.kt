package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.domain.isInlineAudio
import com.exponential.app.domain.isInlineVideo

// Probed attachment dimensions for embedded images (REV2-79). Attachments sync
// their `width`/`height` "so clients can pre-size and avoid layout shift"
// (CLAUDE.md) — web reads them in lib/markdown-image.tsx; this is the Android
// counterpart. Without them an `![](…)` block measured 0-height and jumped the
// layout the moment the bitmap arrived (worst mid-scroll in a long thread).
//
// EXP-824 widens the same map to everything the inline-media block needs from
// the synced row: the content type (video / audio / other decides the
// renderer), the duration chip and whether a poster frame exists.

/** Fallback ratio for an attachment whose row hasn't synced its probe yet —
 *  the same reservation the editor's remote tiles use. */
const val DEFAULT_IMAGE_ASPECT_RATIO: Float = 4f / 3f

/** The aspect box a video with no probed size reserves (EXP-824). */
const val DEFAULT_VIDEO_ASPECT_RATIO: Float = 16f / 9f

/** What the markdown renderers know about one synced attachment row. */
@Immutable
data class AttachmentInfo(
    val width: Int? = null,
    val height: Int? = null,
    val contentType: String? = null,
    val durationMs: Long? = null,
    val hasPoster: Boolean = false,
) {
    val aspectRatio: Float?
        get() {
            val w = width ?: return null
            val h = height ?: return null
            if (w <= 0 || h <= 0) return null
            return w.toFloat() / h.toFloat()
        }

    val isVideo: Boolean get() = isInlineVideo(contentType)
    val isAudio: Boolean get() = isInlineAudio(contentType)

    companion object {
        fun from(row: AttachmentEntity): AttachmentInfo = AttachmentInfo(
            width = row.width,
            height = row.height,
            contentType = row.contentType,
            durationMs = row.durationMs,
            hasPoster = row.hasPoster,
        )
    }
}

/** Attachment id → synced row facts, for the issue currently on screen. */
@Immutable
class AttachmentDims(private val byId: Map<String, AttachmentInfo>) {

    /**
     * Width/height for a markdown image URL, or null when the URL is not one
     * of our attachments (external images keep their natural sizing) or its
     * row hasn't synced yet.
     */
    fun aspectRatioOf(url: String): Float? = infoOf(url)?.aspectRatio

    /** The synced row behind an attachment URL, or null (external / not yet synced). */
    fun infoOf(url: String): AttachmentInfo? {
        val id = attachmentIdFromUrl(url) ?: return null
        return byId[id]
    }

    companion object {
        val Empty = AttachmentDims(emptyMap())

        /** Every synced row of the issue on screen, keyed by id. */
        fun fromRows(rows: List<AttachmentEntity>): AttachmentDims =
            AttachmentDims(rows.associate { it.id to AttachmentInfo.from(it) })
    }
}

/**
 * The attachment id embedded in `/api/attachments/{id}` (the canonical
 * relative form every client stores). Mirrors the web `attachmentIdFromSrc`;
 * returns null for external image URLs. A `?w=480`-style query is tolerated.
 */
fun attachmentIdFromUrl(url: String): String? =
    ATTACHMENT_URL.find(url)?.groupValues?.get(1)

private val ATTACHMENT_URL = Regex("/api/attachments/([^/?#]+)")

/** The poster-frame URL of a media attachment (`/api/attachments/{id}?poster=1`). */
fun attachmentPosterUrl(url: String): String? {
    val id = attachmentIdFromUrl(url) ?: return null
    return "/api/attachments/$id?poster=1"
}

/** Provided by screens that observe the issue's synced attachments. */
val LocalAttachmentDims = compositionLocalOf { AttachmentDims.Empty }
