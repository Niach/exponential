package at.exponential.ui

import at.exponential.ui.compose.codeTokenPart
import at.exponential.ui.compose.markdownCodeHighlighter
import at.exponential.ui.ffi.tokenizeCodeJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.theme.Mode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #VAPP-258 (fixtures/markdown-render.json): a `ts` fence paints CodeBlock's
 * token kinds (the catalog's ONE tokenizer) in the `CodeBlock/token {kind}`
 * recipe's colours; an unknown or empty language is plain. Plain JVM.
 */
class MarkdownCodeTest {
    @Test
    fun aTsFencePaintsCodeBlockTokenColours() {
        val model = makeModel(theme = "neutral", mode = Mode.Light, fixed = true)
        val block = Markdown.parse("```ts\nconst a = 1 // one\n```").single()
        assertEquals("ts", block.lang)
        val text = block.inlines.joinToString("") { it.text }
        val highlighter = markdownCodeHighlighter(model)
        val spans = highlighter.spans(text, block.lang)
        val tokens = JsonValue.parse(tokenizeCodeJson(text, "ts")).array!!.single().array!!
        var at = 0
        val painted = HashSet<String>()
        for (t in tokens) {
            val piece = t["text"]!!.string!!
            val kind = t["kind"]!!.string!!
            if (kind != "plain") {
                val span = spans.single { it.start == at }
                assertEquals(at + piece.length, span.end)
                assertNotNull(kind, model.codeTokenPart(kind).color)
                assertEquals(model.codeTokenPart(kind).color, span.color)
                painted.add(kind)
            }
            at += piece.length
        }
        assertTrue(painted.toString(), painted.containsAll(listOf("keyword", "number", "comment")))
        for (lang in listOf(null, "", "klingon", "plain", "ansi")) assertTrue("$lang", highlighter.spans(text, lang).isEmpty())
    }
}
