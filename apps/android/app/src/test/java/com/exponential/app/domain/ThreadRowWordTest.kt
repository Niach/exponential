package com.exponential.app.domain

import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.SessionTree.ThreadRowWord
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-1309: ONE quiet word per Threads row, off the root's pull requests —
 * replays web's `threads-list.test.ts` (`threadRowWord`) case for case.
 */
class ThreadRowWordTest {

    private fun pr(id: String, state: String, basePullRequestId: String? = null) =
        PullRequestEntity(id = id, state = state, basePullRequestId = basePullRequestId)

    private val open = DomainContract.prStateOpen
    private val merged = DomainContract.prStateMerged

    @Test
    fun `an open PR while nothing works is ready to merge`() {
        assertEquals(ThreadRowWord.ReadyToMerge, SessionTree.threadRowWord(false, false, listOf(pr("a", open))))
        assertNull(SessionTree.threadRowWord(true, false, listOf(pr("a", open))))
    }

    @Test
    fun `an open PR on another open PR (either side) is a stack`() {
        val lower = pr("a", open)
        val upper = pr("b", open, "a")
        assertEquals(ThreadRowWord.Stack, SessionTree.threadRowWord(false, false, listOf(upper), listOf(lower, upper)))
        assertEquals(ThreadRowWord.Stack, SessionTree.threadRowWord(false, false, listOf(lower), listOf(lower, upper)))
        assertEquals(
            ThreadRowWord.ReadyToMerge,
            SessionTree.threadRowWord(false, false, listOf(upper), listOf(pr("a", merged), upper)),
        )
    }

    @Test
    fun `merged wins over a question - a question without a PR`() {
        assertEquals(ThreadRowWord.Merged, SessionTree.threadRowWord(false, true, listOf(pr("a", merged))))
        assertEquals(ThreadRowWord.Question, SessionTree.threadRowWord(false, true, emptyList()))
        assertNull(SessionTree.threadRowWord(false, false, emptyList()))
    }

    @Test
    fun `the words read like web`() {
        assertEquals(
            listOf("ready to merge", "stack", "merged", "question", "needs input"),
            ThreadRowWord.entries.map { it.text },
        )
    }
}
