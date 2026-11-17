package at.exponential.ui

import at.exponential.ui.measure.highlightSpec
import at.exponential.ui.primitives.HIGHLIGHT_BARRIER
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlock
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownChip
import at.exponential.ui.primitives.MarkdownInline
import at.exponential.ui.primitives.highlightLocalCurrent
import at.exponential.ui.primitives.highlightRanges
import at.exponential.ui.primitives.markdownHighlightCount
import at.exponential.ui.primitives.markdownHighlightGroups
import at.exponential.ui.primitives.markdownHighlightText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/** #EDE-95: `fixtures/text-highlight.json` (TS, the Rust core and Swift replay the same cases). */
class TextHighlightTest {
    private val fixture by lazy { Fixtures.json("text-highlight.json") }

    @Test
    fun cases() {
        Fixtures.require()
        val cases = fixture["cases"]?.array.orEmpty()
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val name = c["name"]?.string
            val text = c["text"]!!.string!!
            val ranges = highlightRanges(text, c["query"]!!.string!!)
            val expected = c["ranges"]!!.array!!.map { r -> r.array!!.map { it.number!!.toInt() } }
            assertEquals(name, expected, ranges.map { listOf(it.first, it.last + 1) })
            assertEquals(name, c["matches"]!!.array!!.map { it.string }, ranges.map { text.substring(it.first, it.last + 1) })
        }
    }

    @Test
    fun currentCases() {
        Fixtures.require()
        val cases = fixture["currentCases"]?.array.orEmpty()
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val spec = highlightSpec(c["highlight"])!!
            assertEquals(c["name"]?.string, c["local"]!!.number!!.toInt(), highlightLocalCurrent(spec))
        }
    }

    @Test
    fun markdownCases() {
        Fixtures.require()
        val cases = fixture["markdownCases"]?.array.orEmpty()
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val name = c["name"]?.string
            val query = c["query"]!!.string!!
            val blocks = Markdown.parse(c["text"]!!.string!!)
            assertEquals(name, c["count"]!!.number!!.toInt(), markdownHighlightCount(blocks, query))
            val matches = markdownHighlightGroups(blocks).flatMap { group ->
                val t = markdownHighlightText(group)
                highlightRanges(t, query).map { t.substring(it.first, it.last + 1) }
            }
            assertEquals(name, c["matches"]!!.array!!.map { it.string }, matches)
        }
    }

    /** A chip is one barrier no query matches, so no match crosses it. */
    @Test
    fun chipsAreBarriers() {
        val chip = MarkdownInline("#EXP-1", chip = MarkdownChip("issue", "EXP-1"))
        val block = MarkdownBlock(MarkdownBlockKind.Paragraph, listOf(MarkdownInline("see "), chip, MarkdownInline(" exp")))
        assertEquals("see $HIGHLIGHT_BARRIER exp", markdownHighlightText(block.inlines))
        assertEquals(1, markdownHighlightCount(listOf(block), "exp"))
        assertEquals(0, markdownHighlightCount(listOf(block), "$HIGHLIGHT_BARRIER"))
    }

    /** The prop: an object with a query that folds to something. */
    @Test
    fun spec() {
        assertNull(highlightSpec(null))
        assertNull(highlightSpec(at.exponential.ui.json.JsonValue.parse("""{"query": ""}""")))
        assertNull(highlightSpec(at.exponential.ui.json.JsonValue.parse("""{"query": "́"}""")))
        assertNull(highlightSpec(at.exponential.ui.json.JsonValue.parse(""""a"""")))
        assertEquals(1, highlightSpec(at.exponential.ui.json.JsonValue.parse("""{"query": "a", "current": 1.7}"""))!!.current)
    }
}
