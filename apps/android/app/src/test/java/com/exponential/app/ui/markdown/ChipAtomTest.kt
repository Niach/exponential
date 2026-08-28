package com.exponential.app.ui.markdown

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * A resolved `#IDENTIFIER` chip is ONE thing to the caret (EXP-655): a caret
 * moved into it skips to the edge it was heading for, and a backspace at its
 * right edge removes the whole token (iOS `chipAtomRange` parity). Everything
 * else the field reports passes through [atomizeChipEdit] untouched.
 */
class ChipAtomTest {

    // "see #EXP-238 now" — the token spans [4, 12).
    private val source = "see #EXP-238 now"
    private val chips = IssueChipTransform.build(
        source,
        marks = emptyList(),
        issueRefs = IssueRefHandler(
            candidates = listOf(IssueRefTarget("id", "EXP-238", "mobile chips broken")),
            onOpen = {},
        ),
        paragraphs = listOf(ParagraphAttrs.PLAIN),
    ).chips

    private fun at(caret: Int, text: String = source) = TextFieldValue(text, TextRange(caret))

    @Test
    fun movingRightIntoTheTokenLandsAfterIt() {
        assertEquals(12, snapCaretOutOfChips(chips, oldCaret = 4, newCaret = 5))
        assertEquals(TextRange(12), atomizeChipEdit(at(4), at(5), chips).selection)
    }

    @Test
    fun movingLeftIntoTheTokenLandsBeforeIt() {
        assertEquals(4, snapCaretOutOfChips(chips, oldCaret = 12, newCaret = 11))
        assertEquals(TextRange(4), atomizeChipEdit(at(12), at(11), chips).selection)
    }

    @Test
    fun theEdgesAndTheRestOfTheTextAreLegalPositions() {
        for (caret in listOf(0, 3, 4, 12, 13, 16)) {
            assertEquals(caret, snapCaretOutOfChips(chips, oldCaret = 8, newCaret = caret))
        }
    }

    @Test
    fun aRangeSelectionIsNeverSnapped() {
        val selecting = TextFieldValue(source, TextRange(2, 8))
        assertSame(selecting, atomizeChipEdit(at(2), selecting, chips))
    }

    @Test
    fun backspaceAtTheRightEdgeDeletesTheWholeToken() {
        // The field reports "see #EXP-23 now" with the caret at 11.
        val reported = at(11, "see #EXP-23 now")
        assertEquals(at(4, "see  now"), atomizeChipEdit(at(12), reported, chips))
    }

    @Test
    fun aStaleChipNeverAtomizesTextItNoLongerCovers() {
        // The chips are remembered off the previous text and can be one frame
        // behind when two IME callbacks land together: [4, 12) no longer
        // spells `#EXP-238`, so the backspace stays an ordinary one-character
        // delete instead of splicing out eight innocent characters.
        val stale = "see the words now"
        val reported = at(11, "see the wors now")
        assertSame(reported, atomizeChipEdit(at(12, stale), reported, chips))
    }

    @Test
    fun backspaceElsewhereIsAnOrdinaryDelete() {
        val reported = at(15, "see #EXP-238 no")
        assertSame(reported, atomizeChipEdit(at(16), reported, chips))
    }

    @Test
    fun typingAtTheRightEdgeExtendsTheToken() {
        val reported = at(13, "see #EXP-2389 now")
        assertSame(reported, atomizeChipEdit(at(12), reported, chips))
    }

    @Test
    fun nothingHappensWithoutChips() {
        val reported = at(7)
        assertSame(reported, atomizeChipEdit(at(4), reported, emptyList()))
    }
}
