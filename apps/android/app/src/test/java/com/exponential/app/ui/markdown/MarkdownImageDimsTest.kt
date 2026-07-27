package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * REV2-79: read-mode images pre-size from the SYNCED attachment probe. The
 * markdown carries no dimensions — only `![alt](/api/attachments/{id})` — so
 * the id parsed out of the URL is the whole lookup key (web parity:
 * `attachmentIdFromSrc` in apps/web/src/lib/markdown-image.tsx).
 */
class MarkdownImageDimsTest {

    private val dims = AttachmentDims(
        mapOf(
            "abc-123" to (1600 to 900),
            "square" to (500 to 500),
            "broken" to (0 to 0),
        ),
    )

    @Test
    fun parsesTheAttachmentIdOutOfTheCanonicalUrl() {
        assertEquals("abc-123", attachmentIdFromUrl("/api/attachments/abc-123"))
        // Absolute form (clients resolve to absolute only at fetch time).
        assertEquals(
            "abc-123",
            attachmentIdFromUrl("https://app.exponential.at/api/attachments/abc-123"),
        )
        // The EXP-52 `?w=` display-width param is not part of the id.
        assertEquals("abc-123", attachmentIdFromUrl("/api/attachments/abc-123?w=480"))
    }

    @Test
    fun externalImageUrlsHaveNoAttachmentId() {
        assertNull(attachmentIdFromUrl("https://example.com/cat.png"))
        assertNull(attachmentIdFromUrl("draft://pending"))
    }

    @Test
    fun resolvesTheProbedAspectRatio() {
        assertEquals(1600f / 900f, dims.aspectRatioOf("/api/attachments/abc-123")!!, 0.0001f)
        assertEquals(1f, dims.aspectRatioOf("/api/attachments/square?w=200")!!, 0.0001f)
    }

    @Test
    fun unknownOrDegenerateDimensionsFallThrough() {
        // Not synced yet → caller reserves the 4:3 tile.
        assertNull(dims.aspectRatioOf("/api/attachments/not-synced"))
        // A zero-sized probe must never produce a NaN/Infinity ratio.
        assertNull(dims.aspectRatioOf("/api/attachments/broken"))
        // External images keep their natural sizing.
        assertNull(dims.aspectRatioOf("https://example.com/cat.png"))
        assertNull(AttachmentDims.Empty.aspectRatioOf("/api/attachments/abc-123"))
    }
}
