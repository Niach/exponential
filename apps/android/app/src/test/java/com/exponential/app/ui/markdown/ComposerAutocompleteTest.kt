package com.exponential.app.ui.markdown

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import com.exponential.app.ui.emoji.EmojiRecord
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The composer autocomplete's shared rules (EXP-802/EXP-805). They were the
 * markdown editor's private business until the steer composer needed the same
 * three triggers; both now read them from here, so the precedence and the
 * splices cannot drift between the two composers.
 */
class ComposerAutocompleteTest {

    private fun triggers(beforeCaret: String, mentions: Boolean = true, refs: Boolean = true) =
        autocompleteTriggersAt(beforeCaret, mentionsEnabled = mentions, refsEnabled = refs)

    @Test
    fun mentionWinsOverTheOtherTwo() {
        val t = triggers("ping @an")
        assertEquals("an", t.mentionQuery)
        assertNull(t.issueRefQuery)
        assertNull(t.emoji)
    }

    @Test
    fun issueRefMatchesWhenNoMentionDoes() {
        val t = triggers("see #EXP-8")
        assertNull(t.mentionQuery)
        assertEquals("EXP-8", t.issueRefQuery)
        assertNull(t.emoji)
    }

    @Test
    fun emojiMatchesWhenNeitherTriggerDoes() {
        val t = triggers("nice :tad")
        assertNull(t.mentionQuery)
        assertNull(t.issueRefQuery)
        assertEquals("tad", t.emoji?.query)
    }

    /** A vocabulary that does not exist cannot trigger: no members, no `@` menu. */
    @Test
    fun aDisabledVocabularyNeverTriggers() {
        val t = triggers("ping @an", mentions = false)
        assertTrue(t.none)
        val refs = triggers("see #EXP-8", refs = false)
        assertTrue(refs.none)
    }

    /**
     * The armed latch's reset condition: no token ends at the caret, so the
     * menu closes and only a fresh text change may reopen it (EXP-322).
     */
    @Test
    fun noTokenAtTheCaretIsNone() {
        assertTrue(triggers("plain words ").none)
        assertTrue(triggers("").none)
        // The caret left the token.
        assertTrue(triggers("see #EXP-8 now").none)
    }

    private val members = listOf(
        MentionMember("Ann Example", "ann@example.com"),
        MentionMember("Bo Tester", "bo@test.dev"),
    )

    @Test
    fun mentionCandidatesMatchNameOrEmail() {
        assertEquals(listOf(members[0]), mentionCandidatesFor(members, "ann"))
        assertEquals(listOf(members[1]), mentionCandidatesFor(members, "test.dev"))
        // Case-insensitive, and an empty query offers everyone.
        assertEquals(listOf(members[0]), mentionCandidatesFor(members, "ANN"))
        assertEquals(members, mentionCandidatesFor(members, ""))
    }

    @Test
    fun noQueryMeansNoCandidates() {
        assertEquals(emptyList<MentionMember>(), mentionCandidatesFor(members, null))
    }

    @Test
    fun theLimitCaps() {
        assertEquals(1, mentionCandidatesFor(members, "", limit = 1).size)
    }

    private fun field(text: String) = TextFieldValue(text, TextRange(text.length))

    @Test
    fun aPickedMentionSplicesTheCanonicalForm() {
        val next = field("ping @an").withMention(members[0])
        assertNotNull(next)
        assertEquals("ping @ann@example.com ", next!!.text)
        assertEquals(next.text.length, next.selection.start)
    }

    @Test
    fun aPickedIssueRefSplicesThePlainToken() {
        val next = field("see #EX").withIssueRef(
            IssueRefTarget(issueId = "i1", identifier = "EXP-802", title = "Mentions"),
        )
        assertEquals("see #EXP-802 ", next?.text)
    }

    @Test
    fun aPickedEmojiReplacesTheWholeToken() {
        val record = EmojiRecord(unicode = "🎉", label = "party popper", shortcodes = listOf("tada"))
        assertEquals("ship it 🎉 ", field("ship it :tad").withEmoji(record, trailingSpace = true)?.text)
        // The closing-colon auto-commit is the same splice without the space.
        assertEquals("ship it 🎉", field("ship it :tada:").withEmoji(record, trailingSpace = false)?.text)
    }

    /** A stale row is a no-op, never a mangled splice (EXP-655). */
    @Test
    fun aSpliceWithNoTokenAtTheCaretIsANoOp() {
        assertNull(field("nothing here").withMention(members[0]))
        assertNull(
            field("nothing here").withIssueRef(
                IssueRefTarget(issueId = "i1", identifier = "EXP-802", title = "Mentions"),
            ),
        )
        assertNull(
            field("nothing here").withEmoji(
                EmojiRecord(unicode = "🎉", label = "party popper"),
                trailingSpace = true,
            ),
        )
    }
}
