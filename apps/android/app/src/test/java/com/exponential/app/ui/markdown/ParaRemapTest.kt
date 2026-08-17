package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The paragraph-attribute remap under arbitrary IME edits (EXP-534) — the pure
 * core that lets one multi-line field per text run behave exactly like the old
 * one-field-per-line editor's split/merge intents.
 */
class ParaRemapTest {

    private val plain = ParagraphAttrs.PLAIN
    private val bullet = ParagraphAttrs(kind = BlockKind.ListItem, listType = ListType.Bullet)
    private val ordered = ParagraphAttrs(kind = BlockKind.ListItem, listType = ListType.Ordered, orderedIndex = 1)
    private val checkedItem =
        ParagraphAttrs(kind = BlockKind.ListItem, listType = ListType.Checklist, checked = true)
    private val quote = ParagraphAttrs(kind = BlockKind.Blockquote)
    private val code = ParagraphAttrs(kind = BlockKind.CodeBlock, codeLang = "js")
    private val heading = ParagraphAttrs(kind = BlockKind.Heading, headingLevel = 2)

    private fun remap(
        oldText: String,
        newText: String,
        old: List<ParagraphAttrs>,
        caret: Int? = null,
    ) = ParaRemap.remap(TextDiff.of(oldText, newText, caret), oldText, newText, old)

    // --- geometry helpers ---------------------------------------------------

    @Test
    fun paraIndexAtCountsNewlinesBeforeTheOffset() {
        val text = "a\nbb\nc"
        assertEquals(0, ParaRemap.paraIndexAt(text, 0))
        assertEquals(0, ParaRemap.paraIndexAt(text, 1))
        assertEquals(1, ParaRemap.paraIndexAt(text, 2))
        assertEquals(1, ParaRemap.paraIndexAt(text, 4))
        assertEquals(2, ParaRemap.paraIndexAt(text, 5))
        assertEquals(2, ParaRemap.paraIndexAt(text, 6))
        assertEquals(2, ParaRemap.paraIndexAt(text, 99)) // coerced
    }

    @Test
    fun paragraphBoundsExcludeTheNewline() {
        val text = "a\nbb\nc"
        assertEquals(0 to 1, ParaRemap.paragraphBounds(text, 0))
        assertEquals(2 to 4, ParaRemap.paragraphBounds(text, 1))
        assertEquals(5 to 6, ParaRemap.paragraphBounds(text, 2))
        assertEquals(6 to 6, ParaRemap.paragraphBounds(text, 9)) // past the end
        assertEquals(0 to 0, ParaRemap.paragraphBounds("", 0))
    }

    // --- plain typing -------------------------------------------------------

    @Test
    fun sameLineEditKeepsEveryParagraph() {
        assertEquals(
            listOf(heading, plain),
            remap("T\nbody", "T\nbodyy", listOf(heading, plain), caret = 7),
        )
    }

    // --- Enter --------------------------------------------------------------

    @Test
    fun enterMidLineSplitsKeepingAttrsOnBothHalves() {
        // Splitting a bullet mid-text continues the list on the new line.
        assertEquals(
            listOf(bullet, bullet),
            remap("one", "o\nne", listOf(bullet), caret = 2),
        )
    }

    @Test
    fun enterOnPlainParagraphAddsAPlainLine() {
        assertEquals(
            listOf(plain, plain),
            remap("Hello", "Hello\n", listOf(plain), caret = 6),
        )
    }

    @Test
    fun enterContinuationRules() {
        // checklist continues UNCHECKED, code and quote continue, heading exits.
        assertEquals(
            listOf(checkedItem, checkedItem.copy(checked = false)),
            remap("x", "x\n", listOf(checkedItem), caret = 2),
        )
        assertEquals(listOf(code, code), remap("x", "x\n", listOf(code), caret = 2))
        assertEquals(listOf(quote, quote), remap("x", "x\n", listOf(quote), caret = 2))
        assertEquals(listOf(heading, plain), remap("x", "x\n", listOf(heading), caret = 2))
    }

    @Test
    fun enterAtAmbiguousBoundaryFollowsTheCaret() {
        // "abc\ndef" + '\n' at 3 (end of "abc") and at 4 (start of "def") both
        // yield "abc\n\ndef" — only the caret distinguishes them. The empty
        // line must inherit from the line the caret CAME from.
        assertEquals(
            listOf(heading, plain, bullet),
            remap("abc\ndef", "abc\n\ndef", listOf(heading, bullet), caret = 4),
        )
        assertEquals(
            listOf(heading, bullet, bullet),
            remap("abc\ndef", "abc\n\ndef", listOf(heading, bullet), caret = 5),
        )
    }

    // --- deletion -----------------------------------------------------------

    @Test
    fun deletingANewlineMergesKeepingTheFirstLinesAttrs() {
        assertEquals(
            listOf(heading),
            remap("T\nbody", "Tbody", listOf(heading, plain), caret = 1),
        )
        assertEquals(
            listOf(bullet),
            remap("a\nb", "ab", listOf(bullet, quote), caret = 1),
        )
    }

    @Test
    fun deletingAWholeLineDropsItsAttrs() {
        // Select "x\n" (offsets 2..4) and delete.
        assertEquals(
            listOf(bullet, ordered),
            remap("a\nx\nb", "a\nb", listOf(bullet, quote, ordered), caret = 2),
        )
    }

    // --- paste --------------------------------------------------------------

    @Test
    fun multiLinePasteIntoAListContinuesIt() {
        // Paste "1\n2" at the end of a bullet: line keeps its attrs, each new
        // line continues the list.
        assertEquals(
            listOf(bullet, bullet, bullet),
            remap("a", "a1\n2\n3", listOf(bullet), caret = 6),
        )
    }

    @Test
    fun pasteReplacingASelectionAcrossKindsKeepsTheAnchorLine() {
        // Selection from mid-heading to mid-code replaced by "X\nY": the merged
        // first line keeps the heading, the inserted line continues from it
        // (heading → plain), the code tail line survives.
        assertEquals(
            listOf(heading, plain, code),
            remap(
                "head\nmid\ncode\ntail",
                "heX\nYde\ntail",
                listOf(heading, quote, code, code),
                caret = 5,
            ),
        )
    }

    @Test
    fun driftedParagraphListIsHealedNotThrown() {
        // One entry short: the remap pads with PLAIN instead of crashing.
        assertEquals(
            listOf(bullet, plain, plain),
            remap("a\nb", "a\nb\n", listOf(bullet), caret = 4),
        )
    }
}
