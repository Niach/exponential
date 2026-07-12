package com.exponential.app.data.api

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the wire format of the image-upload multipart body (EXP-61): the server
 * (Bun `request.formData()`) resolves the part by the `name` attribute of its
 * Content-Disposition header and REQUIRES the quoted form — ktor's
 * MultiPartFormDataContent emits the token form (`name=file`), which Bun
 * mis-parses into the key `file; filename=` → HTTP 400 "Missing image file"
 * on every upload. Hence the hand-rolled body in [buildImageUploadBody].
 */
class MultipartWireFormatTest {

    @Test
    fun filePartUsesTheBrowserCanonicalQuotedDisposition() {
        val payload = ByteArray(16) { it.toByte() }
        val (body, boundary) = buildImageUploadBody(payload, "shot.png", "image/png")
        val wire = String(body, Charsets.ISO_8859_1)

        val dispositionLines = wire.lines().filter { it.startsWith("Content-Disposition:") }
        assertEquals(wire, 1, dispositionLines.size)
        assertEquals(
            "Content-Disposition: form-data; name=\"file\"; filename=\"shot.png\"",
            dispositionLines.single(),
        )
        assertTrue(wire, wire.startsWith("--$boundary\r\n"))
        assertTrue(wire, wire.endsWith("\r\n--$boundary--\r\n"))
        // Payload bytes ride through untouched between the blank line and tail.
        val headEnd = wire.indexOf("\r\n\r\n") + 4
        val tailStart = wire.length - "\r\n--$boundary--\r\n".length
        assertEquals(payload.toList(), body.copyOfRange(headEnd, tailStart).toList())
    }

    @Test
    fun headerBreakingFilenameCharactersAreNeutralized() {
        val (body, _) = buildImageUploadBody(
            ByteArray(1),
            "a\"b\\c\r\nd.png",
            "image/png",
        )
        val wire = String(body, Charsets.ISO_8859_1)
        val disposition = wire.lines().first { it.startsWith("Content-Disposition:") }
        assertEquals("Content-Disposition: form-data; name=\"file\"; filename=\"a_b_c__d.png\"", disposition)
        // The mangled filename must not smuggle extra header lines.
        assertFalse(wire, wire.contains("\r\nd.png"))
    }
}
