package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The empty-line tap target (EXP-608). Empty source lines render as a lone
 * zero-width space (the '\n' substitution in [ChipVisualTransformation]), and
 * platform hit-testing resolves a tap past the zero-width glyph — to the
 * offset after it, which belongs to the NEXT paragraph — so the field placed
 * the caret one line below and an empty FIRST line could never take the caret
 * at all. [emptyDisplayLineStart] is the interception rule: the line-start
 * offset for exactly those lines, null for every other line.
 */
class EmptyLineTapTest {

    // Display text for source "\nfoo" — the unreachable empty first line.
    private val emptyFirst = "\u200Bfoo"

    @Test
    fun emptyFirstLineTargetsItsStart() {
        assertEquals(0, emptyDisplayLineStart(emptyFirst, lineStart = 0, lineEnd = 1))
    }

    @Test
    fun nonEmptyLineIsLeftToDefaultHitTesting() {
        // Line 1 of "\u200Bfoo" spans [1, 4).
        assertNull(emptyDisplayLineStart(emptyFirst, lineStart = 1, lineEnd = 4))
    }

    @Test
    fun emptyMiddleLineTargetsItsStart() {
        // Source "a\n\nb" displays as "a\u200B\u200Bb"; the empty middle line
        // is the second ZWSP, spanning [2, 3).
        assertEquals(2, emptyDisplayLineStart("a\u200B\u200Bb", lineStart = 2, lineEnd = 3))
    }

    @Test
    fun singleCharacterLineIsNotAnEmptyLine() {
        // Source "a\nb" displays as "a\u200Bb"; line 1 ("b") spans [2, 3).
        assertNull(emptyDisplayLineStart("a\u200Bb", lineStart = 2, lineEnd = 3))
    }

    @Test
    fun realTrailingEmptyLineIsLeftToDefaultHitTesting() {
        // Source "foo\n" keeps its final newline REAL (EXP-567): the trailing
        // empty line has lineStart == lineEnd and default hit testing already
        // places the caret there correctly.
        assertNull(emptyDisplayLineStart("foo\n", lineStart = 4, lineEnd = 4))
    }
}
