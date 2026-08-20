package com.exponential.app.ui.markdown

import androidx.compose.ui.text.AnnotatedString
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The editor's whole-run visual transformation (EXP-534). Paragraph breaks in
 * the display come from per-line ParagraphStyle ranges over substituted
 * zero-width spaces — EXCEPT a trailing empty line, which only exists in the
 * layout when the final '\n' stays a REAL line break (EXP-567: substituted,
 * the layout had no last line, so the first Enter at the end of a run never
 * moved the caret and the new line only popped in once a character landed on
 * it, with the character on yet another line).
 */
class ChipVisualTransformationTest {

    private fun filter(text: String): AnnotatedString {
        val vt = ChipVisualTransformation(
            marks = emptyList(),
            issueRefs = null,
            paragraphs = List(ParaRemap.lineCount(text)) { ParagraphAttrs.PLAIN },
            fontScale = 1f,
        )
        return vt.filter(AnnotatedString(text)).text
    }

    @Test
    fun interiorNewlinesRenderAsZeroWidthSpaces() {
        assertEquals("a​b", filter("a\nb").text)
    }

    // EXP-567: Enter at the end of the run must yield a real trailing line
    // break, or the layout has no last line for the caret to land on.
    @Test
    fun aFinalTrailingNewlineStaysARealLineBreak() {
        assertEquals("abc\n", filter("abc\n").text)
    }

    @Test
    fun onlyTheFinalNewlineOfTrailingBlankLinesStaysReal() {
        assertEquals("abc​\n", filter("abc\n\n").text)
    }

    @Test
    fun typingOntoTheTrailingLineReSubstitutesItsNewline() {
        assertEquals("abc​x", filter("abc\nx").text)
    }

    /**
     * The trailing empty line has no display character of its own — it must
     * ride the PREVIOUS line's paragraph range as that paragraph's final real
     * '\n' (StaticLayout renders the empty line after it), so the previous
     * range has to include the newline.
     */
    @Test
    fun theTrailingNewlineIsInsideThePreviousParagraphRange() {
        val ranges = filter("abc\n").paragraphStyles
        assertEquals(1, ranges.size)
        assertEquals(0, ranges[0].start)
        assertEquals(4, ranges[0].end)
    }

    @Test
    fun aNewlineOnlyRunKeepsItsRealBreak() {
        assertEquals("\n", filter("\n").text)
    }

    @Test
    fun emptyTextIsUntouched() {
        assertEquals("", filter("").text)
    }
}
