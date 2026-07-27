package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-297: the inline-image classification is a cross-client contract (an
// exact mirror of the server's acceptedImageContentTypes) — anything else has
// to land in the Files section, and cache filenames must never carry a path.
class AttachmentFilesTest {

    @Test
    fun theFiveRasterTypesAreInlineImages() {
        assertEquals(
            setOf("image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"),
            INLINE_IMAGE_CONTENT_TYPES,
        )
        for (type in INLINE_IMAGE_CONTENT_TYPES) {
            assertTrue(type, isInlineImage(type))
        }
    }

    @Test
    fun otherImageTypesAreFiles() {
        assertFalse(isInlineImage("image/tiff"))
        assertFalse(isInlineImage("image/svg+xml"))
        assertFalse(isInlineImage("image/bmp"))
        assertFalse(isInlineImage("application/pdf"))
        assertFalse(isInlineImage("video/mp4"))
        assertFalse(isInlineImage(null))
        assertFalse(isInlineImage(""))
    }

    @Test
    fun classificationIsAnExactMatchLikeEveryOtherClient() {
        // Non-canonical stored types are Files rows on server/web/desktop —
        // classifying them inline here would hide them on Android only.
        assertFalse(isInlineImage("IMAGE/PNG"))
        assertFalse(isInlineImage("image/jpeg; charset=binary"))
        assertFalse(isInlineImage("  image/webp  "))
    }

    @Test
    fun canonicalContentTypeNormalizesPickerTypes() {
        assertEquals("image/png", canonicalContentType("IMAGE/PNG"))
        assertEquals("image/jpeg", canonicalContentType("image/jpeg; charset=binary"))
        assertEquals("image/webp", canonicalContentType("  image/webp  "))
        assertEquals("application/octet-stream", canonicalContentType(null))
        assertEquals("application/octet-stream", canonicalContentType("  ;foo=bar"))
    }

    @Test
    fun fileCapMatchesTheServer() {
        assertEquals(52_428_800L, MAX_FILE_UPLOAD_BYTES)
    }

    @Test
    fun sanitizeStripsPathSeparatorsAndControlChars() {
        assertEquals(".._.._etc_passwd", sanitizeFilename("../../etc/passwd"))
        assertEquals("a_b.txt", sanitizeFilename("a\\b.txt"))
        assertEquals("keeps spaces.pdf", sanitizeFilename("keeps spaces.pdf"))
        assertEquals("no_newline.zip", sanitizeFilename("no\nnewline.zip"))
    }

    @Test
    fun sanitizeFallsBackAndClamps() {
        assertEquals("file", sanitizeFilename(null))
        assertEquals("file", sanitizeFilename("   "))
        assertEquals("file", sanitizeFilename("."))
        assertEquals("file", sanitizeFilename(".."))
        assertEquals(120, sanitizeFilename("x".repeat(400)).length)
    }

    @Test
    fun sanitizeKeepsOrdinaryNames() {
        assertEquals("Q3 report (final).pdf", sanitizeFilename("Q3 report (final).pdf"))
    }
}
