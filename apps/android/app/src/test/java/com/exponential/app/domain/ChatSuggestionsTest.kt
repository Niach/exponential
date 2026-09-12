package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-820: the chat suggestion pool and where a pick leaves the caret. */
class ChatSuggestionsTest {

    /** The pool is hand-mirrored ×4; its SIZE and the `#` placeholders are what
     *  a drift would break first. */
    @Test
    fun `the pool is the shared sixteen`() {
        assertEquals(16, ChatSuggestions.POOL.size)
        assertEquals(ChatSuggestions.POOL.size, ChatSuggestions.POOL.distinct().size)
        assertEquals("Fix #", ChatSuggestions.POOL.first())
        assertTrue(ChatSuggestions.POOL.none { it.isBlank() })
    }

    @Test
    fun `a mount draws distinct suggestions in pool order`() {
        // A fixed draw: always the first candidate of each partial shuffle.
        val picked = ChatSuggestions.pick(random = { 0.0 })
        assertEquals(ChatSuggestions.COUNT, picked.size)
        assertEquals(picked.distinct(), picked)
        assertEquals(picked.sortedBy { ChatSuggestions.POOL.indexOf(it) }, picked)
        // A draw at the other end of the range stays inside the pool.
        val last = ChatSuggestions.pick(random = { 0.999 })
        assertEquals(ChatSuggestions.COUNT, last.size)
        assertTrue(last.all { it in ChatSuggestions.POOL })
        // Asking for more than there is gives the whole pool, not a crash.
        assertEquals(ChatSuggestions.POOL.size, ChatSuggestions.pick(99) { 0.5 }.size)
    }

    /** The `#` rule: the caret lands right BEHIND the `#`, wherever it sits, so
     *  the issue-ref autocomplete opens on the spot. */
    @Test
    fun `the caret lands behind a hash placeholder`() {
        assertEquals(5, ChatSuggestions.caretOffset("Fix #"))
        assertEquals(7, ChatSuggestions.caretOffset("Split # into sub-issues"))
        val complete = "Label every issue in the backlog"
        assertEquals(complete.length, ChatSuggestions.caretOffset(complete))
    }
}
