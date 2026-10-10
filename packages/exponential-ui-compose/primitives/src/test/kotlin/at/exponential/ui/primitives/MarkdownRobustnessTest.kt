package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-103: hostile inline markdown parses in linear time without deep
 * recursion (unclosed link destinations, bracket floods, emphasis floods),
 * link labels nest at most [Markdown.MAX_LINK_DEPTH] deep, and ordinary
 * links still parse.
 */
class MarkdownRobustnessTest {
    private fun fast(name: String, text: String): List<MarkdownInline> {
        val t0 = System.nanoTime()
        val out = Markdown.parseInline(text)
        val ms = (System.nanoTime() - t0) / 1_000_000
        assertTrue("$name took $ms ms", ms < 1_500)
        return out
    }

    @Test
    fun unclosedDestinationsStayLinear() {
        val text = "[a](".repeat(50_000)
        val out = fast("[a]( x 50k", text)
        assertEquals(text, Markdown.plain(out))
        assertTrue(out.all { it.link == null })
    }

    @Test
    fun aBracketFloodStaysLinear() {
        val text = "[".repeat(100_000)
        assertEquals(text, Markdown.plain(fast("[ x 100k", text)))
        val closers = "[".repeat(50_000) + "]".repeat(50_000)
        assertEquals(closers, Markdown.plain(fast("[]", closers)))
        val emphasis = "*a".repeat(50_000) + "_b".repeat(50_000)
        assertTrue(Markdown.plain(fast("emphasis", emphasis)).isNotEmpty())
    }

    @Test
    fun nestedLabelsParseWithinTheCap() {
        val text = "[".repeat(40) + "x" + "]".repeat(40) + "(u)"
        val out = fast("nested", text)
        assertEquals("u", out.single().link)
        assertEquals("[".repeat(39) + "x" + "]".repeat(39), out.single().text)
    }

    @Test
    fun aDestinationAfterAnUnclosedOneStillParses() {
        // The first destination never closes (its own `(` opens); the second link inside it does.
        val out = fast("rescan", "[a](b[c](d)")
        assertEquals("[a](b", out[0].text)
        assertEquals(null, out[0].link)
        assertEquals(MarkdownInline(text = "c", link = "d"), out[1])
        val balanced = fast("balanced", "[a](x(y) [b](A_(b)) [c](\\)z)")
        assertEquals(listOf("A_(b)", ")z"), balanced.mapNotNull { it.link })
    }

    @Test
    fun ordinaryLinksStillParse() {
        val out = Markdown.parseInline("see [the docs](https://x.example/a_(b)) and **bold [b](/c)**")
        assertEquals(MarkdownInline(text = "the docs", link = "https://x.example/a_(b)"), out[1])
        assertEquals(MarkdownInline(text = "b", bold = true, link = "/c"), out.last())
        assertEquals(MarkdownBlockKind.Image("/p.png", "alt"), Markdown.parse("![alt](/p.png)").single().kind)
    }
}
