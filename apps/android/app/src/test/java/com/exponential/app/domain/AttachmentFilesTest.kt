package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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

    // EXP-1247: an as_file upload is never inline and always a Files row
    // (web attachment-files.test "row-aware classification").
    @Test
    fun anAsFileRowIsAFilesRowWhateverItsType() {
        assertTrue(isInlineImageAttachment("image/png", asFile = false))
        assertFalse(isInlineImageAttachment("image/png", asFile = true))
        assertTrue(isInlineMediaAttachment("video/mp4", asFile = false))
        assertFalse(isInlineMediaAttachment("video/mp4", asFile = true))
        assertFalse(isFileAttachment("image/png", asFile = false))
        assertFalse(isFileAttachment("audio/mpeg", asFile = false))
        assertTrue(isFileAttachment("image/png", asFile = true))
        assertTrue(isFileAttachment("video/mp4", asFile = true))
        assertTrue(isFileAttachment("application/pdf", asFile = false))
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

    // EXP-824: video / audio is inline MEDIA, never an inline image — it
    // leaves the Files rail through its own classification.
    @Test
    fun videoAndAudioAreInlineMediaNotImages() {
        assertTrue(isInlineVideo("video/mp4"))
        assertTrue(isInlineVideo("video/quicktime"))
        assertTrue(isInlineAudio("audio/mpeg"))
        assertTrue(isInlineMedia("video/webm"))
        assertTrue(isInlineMedia("audio/ogg"))
        assertFalse(isInlineImage("video/mp4"))
        assertFalse(isInlineImage("audio/mpeg"))
        assertFalse(isInlineMedia("image/png"))
        assertFalse(isInlineMedia("application/pdf"))
        assertFalse(isInlineMedia(null))
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

    // EXP-1003: the web's isMarkdownAttachment table (attachment-files.test.ts),
    // replicated on every client.
    @Test
    fun markdownAttachmentMatchesTheWebTable() {
        val cases = listOf(
            Triple("text/markdown", "notes.md", true),
            Triple("text/markdown; charset=utf-8", "x", true),
            Triple("text/x-markdown", "x.txt", true),
            Triple("", "README.md", true),
            Triple("application/octet-stream", "a.MD", true),
            Triple("text/plain", "spec.markdown", true),
            Triple("text/plain", "spec.txt", false),
            Triple("application/pdf", "spec.md", false),
            Triple("image/png", "x.md", false),
            Triple("text/csv", "x.md", false),
        )
        for ((type, name, expected) in cases) {
            assertEquals("$type / $name", expected, isMarkdownAttachment(type, name))
        }
    }

    @Test
    fun markdownAttachmentToleratesMissingFields() {
        assertTrue(isMarkdownAttachment(null, "a.md"))
        assertTrue(isMarkdownAttachment("text/markdown", null))
        assertTrue(isMarkdownAttachment("  TEXT/Markdown  ", "x"))
        assertTrue(isMarkdownAttachment("", "  notes.md  "))
        assertFalse(isMarkdownAttachment(null, null))
    }

    @Test
    fun markdownPreviewCeilingIsOneMebibyte() {
        assertEquals(1_048_576L, MARKDOWN_PREVIEW_MAX_BYTES)
    }

    @Test
    fun precheckRefusesOnlyRowsOverTheCeiling() {
        assertEquals(
            MarkdownPreviewState.TooLarge,
            markdownPreviewPrecheck(MARKDOWN_PREVIEW_MAX_BYTES + 1),
        )
        assertNull(markdownPreviewPrecheck(MARKDOWN_PREVIEW_MAX_BYTES))
        // Legacy rows carry size_bytes = 0: fetch, and let the body decide.
        assertNull(markdownPreviewPrecheck(0))
    }

    @Test
    fun outcomeChecksTheFetchedText() {
        assertEquals(
            MarkdownPreviewState.Ready("# Hi"),
            markdownPreviewOutcome("# Hi".toByteArray()),
        )
        val max = MARKDOWN_PREVIEW_MAX_BYTES.toInt()
        assertTrue(
            markdownPreviewOutcome(ByteArray(max) { 'a'.code.toByte() })
                is MarkdownPreviewState.Ready,
        )
        assertEquals(
            MarkdownPreviewState.TooLarge,
            markdownPreviewOutcome(ByteArray(max + 1) { 'a'.code.toByte() }),
        )
    }

    @Test
    fun invalidUtf8StillRenders() {
        assertEquals(
            MarkdownPreviewState.Ready("a\uFFFD"),
            markdownPreviewOutcome(byteArrayOf('a'.code.toByte(), 0xFF.toByte())),
        )
    }

    @Test
    fun httpErrorsUseTheWebCopy() {
        assertEquals("This file is no longer available.", markdownPreviewHttpError(404))
        assertEquals("Couldn't load this file (HTTP 500).", markdownPreviewHttpError(500))
    }
}
