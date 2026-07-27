package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf

// Probed attachment dimensions for embedded images (REV2-79). Attachments sync
// their `width`/`height` "so clients can pre-size and avoid layout shift"
// (CLAUDE.md) — web reads them in lib/markdown-image.tsx; this is the Android
// counterpart. Without them an `![](…)` block measured 0-height and jumped the
// layout the moment the bitmap arrived (worst mid-scroll in a long thread).

/** Fallback ratio for an attachment whose row hasn't synced its probe yet —
 *  the same reservation the editor's remote tiles use. */
const val DEFAULT_IMAGE_ASPECT_RATIO: Float = 4f / 3f

/** Attachment id → probed pixel size, for the issue currently on screen. */
@Immutable
class AttachmentDims(private val byId: Map<String, Pair<Int, Int>>) {

    /**
     * Width/height for a markdown image URL, or null when the URL is not one
     * of our attachments (external images keep their natural sizing) or its
     * row hasn't synced yet.
     */
    fun aspectRatioOf(url: String): Float? {
        val id = attachmentIdFromUrl(url) ?: return null
        val (w, h) = byId[id] ?: return null
        if (w <= 0 || h <= 0) return null
        return w.toFloat() / h.toFloat()
    }

    companion object {
        val Empty = AttachmentDims(emptyMap())
    }
}

/**
 * The attachment id embedded in `/api/attachments/{id}` (the canonical
 * relative form every client stores). Mirrors the web `attachmentIdFromSrc`;
 * returns null for external image URLs.
 */
fun attachmentIdFromUrl(url: String): String? =
    ATTACHMENT_URL.find(url)?.groupValues?.get(1)

private val ATTACHMENT_URL = Regex("/api/attachments/([^/?#]+)")

/** Provided by screens that observe the issue's synced attachments. */
val LocalAttachmentDims = compositionLocalOf { AttachmentDims.Empty }
