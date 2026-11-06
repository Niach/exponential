package at.exponential.ui

import at.exponential.ui.compose.codeTokenPart
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #VAPP-258: [MarkdownPainter.codeHighlighter] = the Markdown leaf's highlighter, for a host painting the primitives itself. */
class MarkdownPainterHighlighterTest {
    @Test
    fun theHostHighlighterIsTheLeafs() {
        for ((id, mode) in listOf("neutral" to Mode.Light, "exponential" to Mode.Dark)) {
            val model = makeModel(theme = id, mode = mode, fixed = true)
            val text = "const a = 1 // one"
            val spans = MarkdownPainter.codeHighlighter(ThemeHandle.builtin(id)!!, mode).spans(text, "ts")
            assertTrue(id, spans.isNotEmpty())
            for (s in spans) assertTrue(s.color.alpha > 0f)
            assertEquals(spans.size, at.exponential.ui.compose.markdownCodeSpans(text, "ts") { model.codeTokenPart(it).color }.size)
        }
    }
}
