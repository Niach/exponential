package com.exponential.app.ui.markdown

import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.emoji.matchEmojiToken
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The `@`/`#` autocomplete menu's open rule and placement (EXP-322). The
 * reported bug had two halves: the menu opened whenever the caret merely
 * landed after a `#` token and could not be dismissed, and it was positioned
 * against the whole editor column instead of the caret.
 */
class EditorAutocompleteTest {

    private fun open(
        armed: Boolean = true,
        hasOsFocus: Boolean = true,
        isFocusedRow: Boolean = true,
        kind: BlockKind = BlockKind.Paragraph,
        caretInInlineCode: Boolean = false,
        hasCandidates: Boolean = true,
    ) = shouldOpenAutocomplete(armed, hasOsFocus, isFocusedRow, kind, caretInInlineCode, hasCandidates)

    // --- open rule ----------------------------------------------------------

    @Test
    fun aTextChangeWithCandidatesOpensTheMenu() {
        assertTrue(open())
    }

    /** The regression: tapping into an existing `#EXP-238` must not pop the menu. */
    @Test
    fun caretMovementAloneNeverOpensTheMenu() {
        assertFalse(open(armed = false))
    }

    @Test
    fun anUnfocusedFieldNeverOpensTheMenu() {
        assertFalse(open(hasOsFocus = false))
    }

    @Test
    fun aRowThatIsNotTheModelsFocusedRowNeverOpensTheMenu() {
        assertFalse(open(isFocusedRow = false))
    }

    @Test
    fun codeRowsNeverOpenTheMenu() {
        assertFalse(open(kind = BlockKind.CodeBlock))
    }

    @Test
    fun aCaretInsideInlineCodeNeverOpensTheMenu() {
        assertFalse(open(caretInInlineCode = true))
    }

    @Test
    fun noCandidatesMeansNoMenu() {
        assertFalse(open(hasCandidates = false))
    }

    @Test
    fun inlineCodeDetectionCoversTheWholeSpanInclusive() {
        val marks = listOf(InlineMark(4, 12, InlineKind.InlineCode))
        assertFalse(caretInInlineCode(marks, 3))
        assertTrue(caretInInlineCode(marks, 4))
        assertTrue(caretInInlineCode(marks, 8))
        assertTrue(caretInInlineCode(marks, 12))
        assertFalse(caretInInlineCode(marks, 13))
        assertFalse(caretInInlineCode(listOf(InlineMark(0, 5, InlineKind.Bold)), 2))
    }

    // --- placement ----------------------------------------------------------

    private val window = IntSize(1080, 2400)
    private val menu = IntSize(600, 400)

    private fun place(
        anchor: IntRect = IntRect(40, 500, 1040, 560),
        caretLeft: Int = 100,
        caretTop: Int = 0,
        caretBottom: Int = 60,
        imeBottomPx: Int = 0,
        toolbarHeightPx: Int = 0,
    ) = autocompletePopupOffset(
        anchorBounds = anchor,
        caretLeftInAnchor = caretLeft,
        caretTopInAnchor = caretTop,
        caretBottomInAnchor = caretBottom,
        popupSize = menu,
        windowSize = window,
        imeBottomPx = imeBottomPx,
        toolbarHeightPx = toolbarHeightPx,
        marginPx = 8,
        gapPx = 4,
    )

    @Test
    fun theMenuSitsBelowTheCaretWhenThereIsRoom() {
        assertEquals(IntOffset(140, 564), place())
    }

    @Test
    fun theMenuClampsToTheLeftEdge() {
        assertEquals(8, place(anchor = IntRect(0, 500, 1080, 560), caretLeft = -50).x)
    }

    @Test
    fun theMenuClampsToTheRightEdge() {
        assertEquals(window.width - menu.width - 8, place(caretLeft = 1000).x)
    }

    @Test
    fun theMenuFlipsAboveWhenTheKeyboardEatsTheSpaceBelow() {
        // Caret line at y=1500..1560; the IME takes the bottom 1200px, so the
        // usable band ends at 1192 and the menu must go above the caret.
        val offset = place(anchor = IntRect(40, 1500, 1040, 1560), imeBottomPx = 1200)
        assertEquals(1500 - 4 - menu.height, offset.y)
    }

    @Test
    fun theToolbarHeightShrinksTheUsableBandToo() {
        // Without the toolbar this fits below; with it, it must flip.
        val anchor = IntRect(40, 1500, 1040, 1560)
        assertEquals(1564, place(anchor = anchor, imeBottomPx = 400).y)
        assertEquals(
            1500 - 4 - menu.height,
            place(anchor = anchor, imeBottomPx = 400, toolbarHeightPx = 400).y,
        )
    }

    @Test
    fun theMenuIsPinnedIntoTheBandWhenNeitherSideFits() {
        // Caret near the top with a keyboard covering nearly everything: there
        // is no room above and none below, so it pins rather than drawing
        // behind the keyboard.
        val offset = place(anchor = IntRect(40, 40, 1040, 100), imeBottomPx = 2100)
        assertEquals(8, offset.y)
        assertTrue(offset.y >= 8)
    }

    @Test
    fun theMenuNeverGoesAboveTheTopMargin() {
        for (top in 0..200 step 20) {
            val offset = place(anchor = IntRect(40, top, 1040, top + 60), imeBottomPx = 2000)
            assertTrue("y=${offset.y} for top=$top", offset.y >= 8)
        }
    }

    /**
     * Anchor-relative placement is what makes the menu track scrolling without
     * a scroll listener: shifting the anchor shifts the result by exactly the
     * same delta.
     */
    @Test
    fun placementShiftsExactlyWithTheAnchor() {
        val a = place(anchor = IntRect(40, 500, 1040, 560))
        val b = place(anchor = IntRect(40, 620, 1040, 680))
        assertEquals(a.x, b.x)
        assertEquals(a.y + 120, b.y)
    }

    // --- `:shortcode` emoji trigger (EXP-551) -------------------------------
    //
    // The third trigger in the same family as `@`/`#`, so it lives under the
    // same open rule (armed, focused, never in code). These lock the token
    // SHAPE: only a colon at start-of-text or after whitespace, at least two
    // shortcode characters, an optional closing colon.

    @Test
    fun anEmojiTokenNeedsTwoCharacters() {
        assertNull(matchEmojiToken(":s"))
        assertEquals("sm", matchEmojiToken(":sm")?.query)
        assertEquals("smile", matchEmojiToken("hey :smile")?.query)
    }

    @Test
    fun aClosedEmojiTokenIsReportedAsClosed() {
        val open = matchEmojiToken("go :tada")
        assertEquals(false, open?.closed)
        assertEquals(5, open?.length)
        val closed = matchEmojiToken("go :tada:")
        assertEquals(true, closed?.closed)
        assertEquals(6, closed?.length)
    }

    /** A line start inside a multi-line run counts as whitespace, like `@`/`#`. */
    @Test
    fun aLineStartTriggersToo() {
        assertEquals("sm", matchEmojiToken("first line\n:sm")?.query)
    }

    @Test
    fun clockTimesAndPunctuationNeverTrigger() {
        assertNull(matchEmojiToken("12:30"))
        assertNull(matchEmojiToken("note:"))
        assertNull(matchEmojiToken(":)"))
        assertNull(matchEmojiToken("http://x"))
        assertNull(matchEmojiToken("see https://example.com"))
        assertNull(matchEmojiToken("a:bc"))
    }

    /**
     * Code is inert for every trigger — the emoji auto-commit rides the same
     * gate, which is why it is expressed through [shouldOpenAutocomplete].
     */
    @Test
    fun codeStaysInertForEmojiToo() {
        assertFalse(open(kind = BlockKind.CodeBlock))
        assertFalse(open(caretInInlineCode = true))
    }
}
