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

    // EXP-824: a media upload adds an OPTIONAL `poster` part and plain string
    // fields, every one in the same quoted-disposition form, after the file.

    @Test
    fun mediaPartsRideAfterTheFileInTheQuotedForm() {
        val video = ByteArray(8) { (0x40 + it).toByte() }
        val poster = ByteArray(4) { (0x60 + it).toByte() }
        val (body, boundary) = buildImageUploadBody(
            video,
            "clip.mp4",
            "video/mp4",
            poster = poster,
            fields = mediaUploadFields(width = 1280, height = 720, durationMs = 7345),
        )
        val wire = String(body, Charsets.ISO_8859_1)
        val dispositions = wire.lines().filter { it.startsWith("Content-Disposition:") }
        assertEquals(
            listOf(
                "Content-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"",
                "Content-Disposition: form-data; name=\"poster\"; filename=\"poster.jpg\"",
                "Content-Disposition: form-data; name=\"width\"",
                "Content-Disposition: form-data; name=\"height\"",
                "Content-Disposition: form-data; name=\"durationMs\"",
            ),
            dispositions,
        )
        assertTrue(wire, wire.contains("Content-Type: video/mp4\r\n\r\n" + String(video, Charsets.ISO_8859_1) + "\r\n--$boundary\r\n"))
        assertTrue(wire, wire.contains("Content-Type: image/jpeg\r\n\r\n" + String(poster, Charsets.ISO_8859_1) + "\r\n--$boundary\r\n"))
        assertTrue(wire, wire.contains("name=\"width\"\r\n\r\n1280\r\n--$boundary\r\n"))
        assertTrue(wire, wire.contains("name=\"height\"\r\n\r\n720\r\n--$boundary\r\n"))
        assertTrue(wire, wire.contains("name=\"durationMs\"\r\n\r\n7345\r\n--$boundary--\r\n"))
        assertTrue(wire, wire.endsWith("\r\n--$boundary--\r\n"))
        // Every part opens on the same boundary: file + poster + 3 fields.
        assertEquals(5, Regex("--$boundary\r\n").findAll(wire).count())
    }

    @Test
    fun mediaFieldsSendOnlyPositiveIntegers() {
        assertEquals(emptyMap<String, String>(), mediaUploadFields(null, null, null))
        assertEquals(emptyMap<String, String>(), mediaUploadFields(0, -1, 0))
        assertEquals(
            listOf("width" to "640", "durationMs" to "1"),
            mediaUploadFields(640, null, 1).toList(),
        )
    }

    @Test
    fun noMediaPartsMeansTheOnePartBodyIsByteIdentical() {
        val payload = ByteArray(3) { 1 }
        val (plain, _) = buildImageUploadBody(payload, "a.png", "image/png")
        val (withEmpty, _) = buildImageUploadBody(payload, "a.png", "image/png", poster = null, fields = emptyMap())
        // Boundaries differ per call; compare the shape, not the bytes.
        fun shape(b: ByteArray) = String(b, Charsets.ISO_8859_1).replace(Regex("exp-[0-9a-f-]+"), "B")
        assertEquals(shape(plain), shape(withEmpty))
        assertEquals(1, String(plain, Charsets.ISO_8859_1).lines().count { it.startsWith("Content-Disposition:") })
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
