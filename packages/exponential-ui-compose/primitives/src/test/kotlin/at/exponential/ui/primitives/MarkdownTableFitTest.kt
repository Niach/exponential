package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** #FEED-224: `fixtures/table-fit.json` (TS, the Rust core and Swift replay the same cases). */
class MarkdownTableFitTest {
    @Suppress("UNCHECKED_CAST")
    @Test
    fun fixture() {
        val file = File("../../exponential-ui/fixtures/table-fit.json")
        assumeTrue("$file not in this checkout", file.isFile)
        val f = MiniJson(file.readText()).parse() as Map<String, Any?>
        assertEquals(MarkdownTableFit.WORD_CAP, (f["wordCap"] as Number).toDouble(), 0.0)
        val cases = f["cases"] as List<Map<String, Any?>>
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val columns = (c["columns"] as List<Map<String, Any?>>).map { MarkdownTableFit.Column((it["min"] as Number).toDouble(), (it["max"] as Number).toDouble()) }
            val want = (c["widths"] as List<Any?>).map { (it as Number).toDouble() }
            assertEquals(c["name"] as String, want, MarkdownTableFit.fit(columns, (c["available"] as Number).toDouble()))
        }
    }

    /** #FEED-224: the screenshot's shape fits by the rule, not in equal thirds. */
    @Test
    fun aTableFitsItsColumnsToTheirContent() {
        val s = MarkdownStyles(MarkdownTextSpec(14f, 20f, 400, null))
        val text = object : MarkdownTextMeasure {
            fun plain(inlines: List<MarkdownInline>) = inlines.joinToString("") { it.text }
            override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float =
                (if (width == null) 1f else kotlin.math.max(kotlin.math.ceil(plain(inlines).length * 8f / width), 1f)) * spec.lineHeight
            override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = plain(inlines).length * 8f
            override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = (plain(inlines).split(' ').maxOfOrNull { it.length } ?: 0) * 8f
        }
        val md = "| PR | Issue | Stand |\n|---|---|---|\n| #2229 | MA-1756-Matty-Stufe-2-Vorgabe | Gründe hängt in der Merge-Warteschlange und wartet auf CI |"
        val lay = Markdown.layout(Markdown.parse(md), s, 500f + 6 * s.cellPadH, text)
        assertEquals(listOf(40f, 230f, 230f), lay.blocks[0].cols)
        val short = Markdown.layout(Markdown.parse("| a | b |\n|---|---|\n| x | yy |"), s, 600f, text)
        assertEquals(listOf(8f, 16f), short.blocks[0].cols)
    }
}
