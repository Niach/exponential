package com.exponential.app.ui.issue

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.isViaMcp
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-741: the ONE grouping rule every client's activity feed applies.
class CommentThreadsTest {

    private val ts = "2026-07-01 10:00:00+00"

    private fun row(id: String, parentId: String? = null, source: String? = null) = CommentEntity(
        id = id,
        issueId = "issue-1",
        teamId = "team-1",
        authorId = "author-1",
        parentId = parentId,
        source = source,
        body = "hi",
        createdAt = ts,
        updatedAt = ts,
    )

    @Test
    fun keepsOrderAndGroupsRepliesUnderTheirParent() {
        val threads = threadComments(
            listOf(row("a"), row("a1", "a"), row("b"), row("a2", "a"), row("b1", "b")),
        )
        assertEquals(listOf("a", "b"), threads.topLevel.map { it.id })
        assertEquals(listOf("a1", "a2"), threads.repliesByParent["a"]?.map { it.id })
        assertEquals(listOf("b1"), threads.repliesByParent["b"]?.map { it.id })
        assertEquals(5, threads.count)
    }

    @Test
    fun orphanReplySurfacesAsTopLevel() {
        val threads = threadComments(listOf(row("orphan", "gone"), row("c")))
        assertEquals(listOf("orphan", "c"), threads.topLevel.map { it.id })
        assertTrue(threads.repliesByParent.isEmpty())
    }

    @Test
    fun neverNestsARowUnderItself() {
        assertEquals(listOf("self"), threadComments(listOf(row("self", "self"))).topLevel.map { it.id })
    }

    @Test
    fun viaMcpReadsOffTheSource() {
        assertFalse(row("a").isViaMcp)
        assertTrue(row("m", source = DomainContract.commentSourceMcp).isViaMcp)
    }
}
