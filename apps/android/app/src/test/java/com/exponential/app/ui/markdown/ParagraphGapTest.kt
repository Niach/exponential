package com.exponential.app.ui.markdown

import androidx.compose.ui.text.AnnotatedString
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1676: the editor's paragraph rhythm (web's 0.5rem `p` / task `li`
 * margins). A BasicTextField has no paragraph spacing, so the display injects
 * a zero-width gap char after each '\n' with its own blockSpacing-tall
 * paragraph; it must never be a caret position.
 */
class ParagraphGapTest {

    private val code = ParagraphAttrs(kind = BlockKind.CodeBlock)

    private fun build(
        source: String,
        paragraphs: List<ParagraphAttrs> = List(ParaRemap.lineCount(source)) { ParagraphAttrs.PLAIN },
        handler: IssueRefHandler? = null,
    ) = IssueChipTransform.build(source, emptyList(), handler, paragraphs, paragraphGaps = true)

    @Test
    fun aGapFollowsEveryParagraphBreak() {
        val t = build("ab\ncd")
        assertFalse(t.isIdentity)
        assertEquals("ab\n​cd", t.display)
        assertEquals(listOf(0 to 2, 4 to 6), t.lineBounds())
    }

    @Test
    fun theGapIsNeverACaretPosition() {
        val t = build("ab\ncd")
        // End of line one stays on line one; the next line starts after the gap.
        assertEquals(2, t.caretToDisplay(2))
        assertEquals(4, t.caretToDisplay(3))
        assertEquals(4, t.originalToTransformed(3))
        // A tap on the gap lands at the next line's start.
        assertEquals(3, t.transformedToOriginal(3))
        assertEquals(3, t.transformedToOriginal(4))
        assertEquals(2, t.transformedToOriginal(2))
        assertEquals(5, t.transformedToOriginal(6))
    }

    @Test
    fun codeFenceLinesPack() {
        val t = build("a\nb\nc", listOf(code, code, ParagraphAttrs.PLAIN))
        assertEquals("a\nb\n​c", t.display)
    }

    @Test
    fun anEmptyLastLineOwnsADisplayChar() {
        val t = build("ab\n")
        assertEquals("ab\n​​", t.display)
        assertEquals(listOf(0 to 2, 4 to 5), t.lineBounds())
        assertEquals(5, t.caretToDisplay(3))
        assertEquals(3, t.transformedToOriginal(4))
        assertEquals(3, t.transformedToOriginal(5))
    }

    @Test
    fun gapsAndChipsCompose() {
        val handler = IssueRefHandler(
            candidates = listOf(IssueRefTarget("id-1", "EXP-1", "one")),
            onOpen = {},
        )
        val t = build("#EXP-1\nx #EXP-1", handler = handler)
        assertEquals("#EXP-1 one\n​x #EXP-1 one", t.display)
        assertEquals(2, t.chips.size)
        assertEquals(listOf(0 to 10, 12 to t.display.length), t.lineBounds())
        // Every offset maps in range both ways, and round-trips off the injections.
        for (o in 0..t.source.length) {
            if (t.chips.any { o > it.sourceStart && o < it.sourceEnd }) continue
            val d = t.caretToDisplay(o)
            assertTrue(d in 0..t.display.length)
            assertEquals(o, t.transformedToOriginal(d))
        }
        for (q in 0..t.display.length) assertTrue(t.transformedToOriginal(q) in 0..t.source.length)
    }

    @Test
    fun theGapIsItsOwnParagraph() {
        val vt = ChipVisualTransformation(
            marks = emptyList(),
            issueRefs = null,
            paragraphs = List(2) { ParagraphAttrs.PLAIN },
            fontScale = 1f,
            paragraphGaps = true,
        )
        val out = vt.filter(AnnotatedString("ab\ncd"))
        assertEquals("ab​​cd", out.text.text)
        val ranges = out.text.paragraphStyles.map { it.start to it.end }.sortedBy { it.first }
        assertEquals(listOf(0 to 3, 3 to 4, 4 to 6), ranges)
        assertEquals(4, out.offsetMapping.originalToTransformed(3))
    }

    @Test
    fun singleLineIsIdentity() {
        assertTrue(build("plain").isIdentity)
    }

    @Test
    fun readOnlySoftBreakLinesContinueTheirParagraph() {
        val tb = MarkdownParser.parse("one\ntwo\n\nthree", softBreaksAsNewlines = true)
            .filterIsInstance<ContentBlock.TextBlock>().first { it.content.text.isNotEmpty() }
        assertEquals(listOf(false, true, false), tb.content.paragraphs.take(3).map { it.lineBreak })
        // The editor's parse never marks a line.
        val edit = MarkdownParser.parse("one\n\ntwo").filterIsInstance<ContentBlock.TextBlock>().first()
        assertTrue(edit.content.paragraphs.none { it.lineBreak })
    }
}
