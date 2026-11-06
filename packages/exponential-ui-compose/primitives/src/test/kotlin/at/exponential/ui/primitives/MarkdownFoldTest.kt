package at.exponential.ui.primitives

import kotlin.math.ceil
import org.junit.Assert.assertEquals
import org.junit.Test

/** #VAPP-258: [Markdown.foldHeight] folds at whole lines, never through one. */
class MarkdownFoldTest {
    /** 10 dp per character, so a line holds width / 10 characters. */
    private object Chars : MarkdownTextMeasure {
        override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float {
            val n = Markdown.plain(inlines).length
            val lines = if (width == null) 1 else maxOf(1, ceil(n * 10f / width).toInt())
            return lines * spec.lineHeight
        }
        override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = Markdown.plain(inlines).length * 10f
        override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = 10f
    }

    private val styles = MarkdownStyles(MarkdownTextSpec(size = 14f, lineHeight = 22f, weight = 400, family = null))

    private fun fold(text: String, limit: Float): Float {
        val blocks = Markdown.parse(text)
        return Markdown.foldHeight(blocks, Markdown.layout(blocks, styles, 100f, Chars), styles, limit)
    }

    @Test
    fun foldsAtWholeLines() {
        // One paragraph of 5 lines (22 each): a 63 dp clamp keeps 2 lines (44), not 2.86.
        assertEquals(44f, fold("a".repeat(50), 63f), 0.01f)
        // Short enough: the document's own height.
        assertEquals(22f, fold("short", 63f), 0.01f)
        // Paragraph (1 line) + gap (8) + a long one: its whole lines below the gap.
        assertEquals(22f + 8f + 22f, fold("one\n\n" + "b".repeat(50), 63f), 0.01f)
        // Nothing fits: the first line's bottom, capped at the limit.
        assertEquals(10f, fold("a".repeat(50), 10f), 0.01f)
    }
}
