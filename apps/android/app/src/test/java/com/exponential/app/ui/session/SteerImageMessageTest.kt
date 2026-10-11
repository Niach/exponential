package com.exponential.app.ui.session

import com.exponential.app.domain.SteerFile
import com.exponential.app.domain.buildSteerImageMessage
import com.exponential.app.domain.buildSteerMessage
import com.exponential.app.domain.imageMarker
import com.exponential.app.domain.insertImageMarker
import com.exponential.app.domain.SteerMessageSegment
import com.exponential.app.domain.parseSteerMessage
import com.exponential.app.domain.steerMessageSegments
import com.exponential.app.domain.renumberImageMarkers
import com.exponential.app.domain.steerImageMarkers
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-511: the composed steer message is a cross-client contract — web, iOS and
// Android build the exact same string from the same inputs, and the host's
// reverse rewrite only matches if it stays byte-identical.
class SteerImageMessageTest {

    private val idA = "11111111-1111-4111-8111-111111111111"
    private val idB = "22222222-2222-4222-8222-222222222222"

    @Test
    fun `text and images are joined by a blank line`() {
        assertEquals(
            "fix the header\n\n![image](/api/attachments/$idA)\n![image](/api/attachments/$idB)",
            buildSteerImageMessage("fix the header", listOf(idA, idB)),
        )
    }

    @Test
    fun `blank text yields the embeds alone`() {
        assertEquals(
            "![image](/api/attachments/$idA)",
            buildSteerImageMessage("  \n ", listOf(idA)),
        )
    }

    @Test
    fun `no images leaves the trimmed text unchanged`() {
        assertEquals("hello", buildSteerImageMessage("  hello  ", emptyList()))
    }

    @Test
    fun `empty input stays empty`() {
        assertEquals("", buildSteerImageMessage("", emptyList()))
    }

    // ── EXP-698: the positional half — `[Image #N]` markers ─────────────────

    @Test
    fun `the marker spelling is the contract`() {
        assertEquals("[Image #1]", imageMarker(1))
        assertEquals("[Image #12]", imageMarker(12))
    }

    @Test
    fun `parse is the inverse of compose`() {
        val composed = buildSteerImageMessage("crop [Image #2] please", listOf(idA, idB))
        val parsed = parseSteerMessage(composed)
        assertEquals("crop [Image #2] please", parsed.text)
        assertEquals(listOf(idA, idB), parsed.attachmentIds)
        assertEquals(listOf(2L), parsed.markers)
    }

    @Test
    fun `markers come back in text order, deduped`() {
        val parsed = parseSteerMessage("[Image #2] then [Image #1] then [Image #2] again")
        assertEquals(listOf(2L, 1L), parsed.markers)
    }

    @Test
    fun `a message with no embeds parses as prose`() {
        val parsed = parseSteerMessage("just words")
        assertEquals("just words", parsed.text)
        assertEquals(emptyList<String>(), parsed.attachmentIds)
        assertEquals(emptyList<Long>(), parsed.markers)
    }

    @Test
    fun `insert spaces the marker off the words around it`() {
        val (text, caret) = insertImageMarker("crop this", 4, 1)
        assertEquals("crop [Image #1] this", text)
        // Caret lands right after the marker, before the separating space.
        assertEquals("crop [Image #1]".length, caret)
    }

    @Test
    fun `insert between two words spaces both sides`() {
        val (text, caret) = insertImageMarker("ab", 1, 1)
        assertEquals("a [Image #1] b", text)
        // Behind the whole insertion, INCLUDING the trailing space (web/iOS
        // parity — the caret is where typing continues).
        assertEquals("a [Image #1] ".length, caret)
    }

    @Test
    fun `segments split the prose on its markers`() {
        assertEquals(
            listOf(
                SteerMessageSegment.Text("crop "),
                SteerMessageSegment.Marker(2L),
                SteerMessageSegment.Text(" please"),
            ),
            steerMessageSegments("crop [Image #2] please"),
        )
    }

    @Test
    fun `insert at the end needs no trailing space`() {
        val (text, caret) = insertImageMarker("crop ", 5, 2)
        assertEquals("crop [Image #2]", text)
        assertEquals(text.length, caret)
    }

    @Test
    fun `insert into empty text adds no padding at all`() {
        assertEquals("[Image #1]" to "[Image #1]".length, insertImageMarker("", 0, 1))
    }

    // EXP-698: a marker's digits are parsed ONCE, so the three walkers cannot
    // disagree about what is a marker — `markers` used to drop an oversize one
    // that `renumberImageMarkers` kept verbatim.

    @Test
    fun `an oversize marker is still a marker everywhere`() {
        val big = 99_999_999_999L
        val draft = "see [Image #$big] there"
        assertEquals(listOf(big), steerImageMarkers(draft))
        assertEquals(listOf(big), parseSteerMessage(draft).markers)
        assertEquals(
            listOf(
                SteerMessageSegment.Text("see "),
                SteerMessageSegment.Marker(big),
                SteerMessageSegment.Text(" there"),
            ),
            steerMessageSegments(draft),
        )
        // Higher than the removed index, so it slides down like any other.
        assertEquals("see [Image #${big - 1}] there", renumberImageMarkers(draft, 1))
    }

    @Test
    fun `digits too big to parse are prose everywhere`() {
        val draft = "see [Image #999999999999999999999999] there"
        assertEquals(emptyList<Long>(), steerImageMarkers(draft))
        assertEquals(emptyList<Long>(), parseSteerMessage(draft).markers)
        assertEquals(listOf(SteerMessageSegment.Text(draft)), steerMessageSegments(draft))
        assertEquals(draft, renumberImageMarkers(draft, 1))
    }

    @Test
    fun `removing an image drops its marker and pulls the higher ones down`() {
        val draft = "a [Image #1] b [Image #2] c [Image #3]"
        assertEquals("a b [Image #1] c [Image #2]", renumberImageMarkers(draft, 1))
        assertEquals("a [Image #1] b c [Image #2]", renumberImageMarkers(draft, 2))
        assertEquals("a [Image #1] b [Image #2] c", renumberImageMarkers(draft, 3))
    }

    @Test
    fun `renumber closes the gap the removed marker leaves`() {
        assertEquals("crop this", renumberImageMarkers("crop [Image #1] this", 1))
        assertEquals("crop", renumberImageMarkers("crop [Image #1]", 1))
        assertEquals("this", renumberImageMarkers("[Image #1] this", 1))
    }

    @Test
    fun `renumber leaves an untouched draft alone`() {
        assertEquals("no markers here", renumberImageMarkers("no markers here", 1))
    }

    // ── Wave D: any file rides the message as a plain link line ──────────────

    private val idC = "33333333-3333-4333-8333-333333333333"

    @Test
    fun `files follow the image block, one link line each`() {
        assertEquals(
            "fix [Image #1]\n\n![image](/api/attachments/$idA)\n" +
                "[report.pdf](/api/attachments/$idB)\n[notes [v2\\].txt](/api/attachments/$idC)",
            buildSteerMessage(
                "fix [Image #1]",
                listOf(idA),
                listOf(SteerFile(idB, "report.pdf"), SteerFile(idC, "notes [v2].txt")),
            ),
        )
    }

    @Test
    fun `files without images sit a blank line below the prose`() {
        assertEquals(
            "read this\n\n[a\\\\b.log](/api/attachments/$idB)",
            buildSteerMessage("read this", emptyList(), listOf(SteerFile(idB, "a\\b.log"))),
        )
    }

    @Test
    fun `files alone yield the link lines alone`() {
        assertEquals(
            "[report.pdf](/api/attachments/$idB)",
            buildSteerMessage("  ", emptyList(), listOf(SteerFile(idB, "report.pdf"))),
        )
    }

    @Test
    fun `no files is the frozen image builder`() {
        assertEquals(
            buildSteerImageMessage("fix it", listOf(idA, idB)),
            buildSteerMessage("fix it", listOf(idA, idB), emptyList()),
        )
    }

    @Test
    fun `parse peels the file lines before the image embeds`() {
        val files = listOf(SteerFile(idB, "notes [v2].txt"), SteerFile(idC, "a\\b.log"))
        val parsed = parseSteerMessage(buildSteerMessage("crop [Image #1]", listOf(idA), files))
        assertEquals("crop [Image #1]", parsed.text)
        assertEquals(listOf(idA), parsed.attachmentIds)
        assertEquals(files, parsed.files)
        assertEquals(listOf(1L), parsed.markers)
    }

    @Test
    fun `parse of files without images keeps the prose`() {
        val parsed = parseSteerMessage("read this\n\n[report.pdf](/api/attachments/$idB)")
        assertEquals("read this", parsed.text)
        assertEquals(emptyList<String>(), parsed.attachmentIds)
        assertEquals(listOf(SteerFile(idB, "report.pdf")), parsed.files)
    }

    @Test
    fun `an image-only message parses exactly as before`() {
        val parsed = parseSteerMessage(buildSteerImageMessage("hi", listOf(idA)))
        assertEquals("hi", parsed.text)
        assertEquals(listOf(idA), parsed.attachmentIds)
        assertEquals(emptyList<SteerFile>(), parsed.files)
    }

    @Test
    fun `a unicode space inside an attachment id is not an embed, like web`() {
        val parsed = parseSteerMessage("hi\n\n![image](/api/attachments/a\u00A0b)")
        assertEquals(emptyList<String>(), parsed.attachmentIds)
    }

    // ICU (the Android regex engine) rejects the `(?U)` flag the JVM accepts, so
    // a JVM-green regex crashed the app on first use (2026-10-11).
    @Test
    fun `no main source uses the (?U) regex flag`() {
        val root = java.io.File("src/main")
        val offenders = root.walkTopDown()
            .filter { it.isFile && it.extension == "kt" && it.readText().contains("(?U)") }
            .map { it.name }
            .toList()
        assert(root.isDirectory)
        assertEquals(emptyList<String>(), offenders)
    }
}
