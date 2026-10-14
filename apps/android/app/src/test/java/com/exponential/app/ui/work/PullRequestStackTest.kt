package com.exponential.app.ui.work

import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PullRequestStackTest {
    private fun pr(id: String, number: Int, base: String? = null, state: String = DomainContract.prStateOpen) =
        PullRequestEntity(id = id, number = number, head = "exp/$id", base = "main", basePullRequestId = base, state = state)

    @Test
    fun `a lone open PR has no stack`() {
        assertNull(PullRequestStack.stackConfirm(pr("a", 1), listOf(pr("a", 1))))
    }

    @Test
    fun `merge stack from the middle lands the whole chain through its top`() {
        val all = listOf(pr("t", 3, base = "m"), pr("b", 1), pr("m", 2, base = "b"))
        val confirm = PullRequestStack.stackConfirm(all[2], all)!!
        assertEquals(DomainContract.diffUiMergeStack, confirm.title)
        assertEquals("Lands 3 pull requests, bottom-up: #1, #2, #3.", confirm.body)
        assertEquals("t", confirm.throughId)
    }

    @Test
    fun `merged members drop out of the open chain`() {
        val all = listOf(pr("b", 1, state = DomainContract.prStateMerged), pr("m", 2, base = "b"), pr("t", 3, base = "m"))
        val confirm = PullRequestStack.stackConfirm(all[1], all)!!
        assertEquals("Lands 2 pull requests, bottom-up: #2, #3.", confirm.body)
    }

    @Test
    fun `a fork has no stack`() {
        val all = listOf(pr("b", 1), pr("x", 2, base = "b"), pr("y", 3, base = "b"))
        assertNull(PullRequestStack.stackConfirm(all[0], all))
        assertNull(PullRequestStack.stackConfirm(all[1], all))
    }
}
