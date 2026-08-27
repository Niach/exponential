package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The past-line-end tap target (EXP-608 for empty lines, EXP-655 for short
 * ones). Non-final source newlines render as a zero-width space at the end of
 * their display line (the substitution in [ChipVisualTransformation]), and
 * platform hit-testing resolves a tap past that glyph to the offset AFTER it,
 * which belongs to the NEXT paragraph — so the caret landed one line below
 * and an empty first line could never take the caret at all. [tapCaretOnLine]
 * is the interception rule: the line's own end for exactly those hits, null
 * for everything default hit testing already gets right.
 */
class TapCaretOnLineTest {

    private val zwsp = '\u200B'

    // Display text for source "\nfoo" — the unreachable empty first line.
    private val emptyFirst = "${zwsp}foo"

    @Test
    fun emptyFirstLineTargetsItsStart() {
        // Wherever the hit resolved: past the glyph (1) or on it (0).
        assertEquals(0, tapCaretOnLine(emptyFirst, lineStart = 0, lineEnd = 1, hit = 1))
        assertEquals(0, tapCaretOnLine(emptyFirst, lineStart = 0, lineEnd = 1, hit = 0))
    }

    @Test
    fun emptyMiddleLineTargetsItsStart() {
        // Source "a\n\nb" displays as "a\u200B\u200Bb"; the empty middle line
        // is the second ZWSP, spanning [2, 3).
        assertEquals(2, tapCaretOnLine("a$zwsp${zwsp}b", lineStart = 2, lineEnd = 3, hit = 3))
    }

    /** EXP-655: the free right half of a short line puts the caret at its end. */
    @Test
    fun aHitPastAShortLinesEndTargetsThatLinesEnd() {
        // Source "short\nnext" displays as "short\u200Bnext"; line 0 spans [0, 6).
        val display = "short${zwsp}next"
        assertEquals(5, tapCaretOnLine(display, lineStart = 0, lineEnd = 6, hit = 6))
    }

    @Test
    fun aHitInsideTheGlyphsIsLeftToDefaultHitTesting() {
        val display = "short${zwsp}next"
        assertNull(tapCaretOnLine(display, lineStart = 0, lineEnd = 6, hit = 2))
        assertNull(tapCaretOnLine(display, lineStart = 0, lineEnd = 6, hit = 5))
    }

    @Test
    fun theLastLineHasNoStandInAndIsLeftAlone() {
        // Line 1 of "short\u200Bnext" spans [6, 10) and ends in a real glyph.
        assertNull(tapCaretOnLine("short${zwsp}next", lineStart = 6, lineEnd = 10, hit = 10))
    }

    @Test
    fun singleCharacterLineIsNotAnEmptyLine() {
        // Source "a\nb" displays as "a\u200Bb"; line 1 ("b") spans [2, 3).
        assertNull(tapCaretOnLine("a${zwsp}b", lineStart = 2, lineEnd = 3, hit = 3))
    }

    @Test
    fun realTrailingEmptyLineIsLeftToDefaultHitTesting() {
        // Source "foo\n" keeps its final newline REAL (EXP-567): the trailing
        // empty line has lineStart == lineEnd and default hit testing already
        // places the caret there correctly.
        assertNull(tapCaretOnLine("foo\n", lineStart = 4, lineEnd = 4, hit = 4))
    }
}
