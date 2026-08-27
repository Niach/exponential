package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The autocomplete splice (EXP-655). A pick used to rebuild the trigger's
 * index from a LIVE caret and a query captured at the composition that last
 * rendered the menu; the Popup lags the field by a composition easily, so a
 * caret one character past the captured query kept the typed `#` and wrote
 * `##EXP-552`. The splice now re-matches the trigger against the live text at
 * the caret, so both ends of the replaced range come from one snapshot.
 */
class TriggerSpliceTest {

    private fun issueRef(text: String, caret: Int, identifier: String) =
        spliceTriggerToken(text, caret, ISSUE_REF_AT_CARET, "#$identifier ")

    @Test
    fun bareHashIsReplacedByTheToken() {
        assertEquals(
            "this also relates to #EXP-552 " to 30,
            issueRef("this also relates to #", 22, "EXP-552"),
        )
    }

    @Test
    fun aTypedQueryIsReplacedTogetherWithItsHash() {
        assertEquals("see #EXP-552 now" to 13, issueRef("see #EXnow", 7, "EXP-552"))
    }

    /** The regression: the live text is a character longer than the query the menu row was composed with. */
    @Test
    fun aCaretPastAStaleQueryStillWritesOneHash() {
        val text = "this also relates to #E"
        val (newText, caret) = issueRef(text, text.length, "EXP-552")!!
        assertEquals("this also relates to #EXP-552 ", newText)
        assertEquals(newText.length, caret)
        assertEquals(1, newText.count { it == '#' })
    }

    @Test
    fun aTriggerAtTheStartOfALineSplices() {
        assertEquals("first\n#EXP-1 " to 13, issueRef("first\n#", 7, "EXP-1"))
    }

    @Test
    fun noTriggerAtTheCaretIsANoOp() {
        assertNull(issueRef("plain words", 5, "EXP-1"))
        // The caret left the token: whitespace after it.
        assertNull(issueRef("see #EX now", 11, "EXP-1"))
    }

    @Test
    fun mentionsSpliceTheSameWay() {
        assertEquals(
            "hi @ann@example.com " to 20,
            spliceTriggerToken("hi @an", 6, MENTION_AT_CARET, "@ann@example.com "),
        )
    }

    @Test
    fun emojiShortcodesReplaceTheWholeToken() {
        assertEquals("party 🎉 " to 9, spliceEmojiToken("party :tad", 10, "🎉 "))
        assertEquals("party 🎉" to 8, spliceEmojiToken("party :tada:", 12, "🎉"))
        assertNull(spliceEmojiToken("party time", 10, "🎉"))
    }

    @Test
    fun anOutOfRangeCaretIsCoerced() {
        assertEquals("#EXP-1 " to 7, issueRef("#", 99, "EXP-1"))
    }
}
