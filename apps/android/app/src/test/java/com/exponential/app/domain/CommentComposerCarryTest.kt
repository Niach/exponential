package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

// Release train 2026-10-10, F34: a comment draft and its queued files never
// follow the composer to another issue (the Work screen's Stack swap rebinds
// one screen-level VM); a re-bind to the same issue keeps them.
class CommentComposerCarryTest {

    @Test
    fun `a changed issue id starts the composer empty`() {
        val carry = commentComposerAfterBind("issue-a", "issue-b", "half a thought", listOf("shot.png"))
        assertEquals("", carry.draft)
        assertEquals(emptyList<String>(), carry.pending)
    }

    @Test
    fun `re-binding the same issue keeps the draft and its files`() {
        val carry = commentComposerAfterBind("issue-a", "issue-a", "half a thought", listOf("shot.png"))
        assertEquals("half a thought", carry.draft)
        assertEquals(listOf("shot.png"), carry.pending)
    }

    @Test
    fun `the first bind clears nothing worth keeping`() {
        val carry = commentComposerAfterBind(null, "issue-a", "", emptyList<String>())
        assertEquals(CommentComposerCarry("", emptyList<String>()), carry)
    }
}
