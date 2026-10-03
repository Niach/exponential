package com.exponential.app.ui.issue

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.isFromReporter
import com.exponential.app.data.db.isToReporter
import com.exponential.app.data.db.isViaMcp
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-741: the ONE grouping rule every client's activity feed applies.
class CommentThreadsTest {

    private val ts = "2026-07-01 10:00:00+00"

    private fun row(
        id: String,
        parentId: String? = null,
        source: String? = null,
        audience: String = DomainContract.commentAudienceTeam,
        authorId: String? = "author-1",
    ) = CommentEntity(
        id = id,
        issueId = "issue-1",
        teamId = "team-1",
        authorId = authorId,
        parentId = parentId,
        source = source,
        audience = audience,
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

    // SLOP-4: a reporter's comment (no author row) vs a member's reply the
    // server emailed out — the two captions beside "via MCP".
    @Test
    fun reporterFlagsReadOffSourceAndAudience() {
        val fromReporter = row(
            "r", source = DomainContract.commentSourceReporter,
            audience = DomainContract.commentAudienceReporter, authorId = null,
        )
        assertTrue(fromReporter.isFromReporter)
        assertFalse(fromReporter.isToReporter)
        assertFalse(fromReporter.isViaMcp)

        val toReporter = row("t", audience = DomainContract.commentAudienceReporter)
        assertFalse(toReporter.isFromReporter)
        assertTrue(toReporter.isToReporter)

        assertFalse(row("a").isFromReporter)
        assertFalse(row("a").isToReporter)
    }

    // A reporter's row decodes with its NULL author and its audience; an
    // older server's row without `audience` reads as a team comment.
    @Test
    fun decodesReporterRowAndDefaultsAudience() {
        val json = kotlinx.serialization.json.Json { ignoreUnknownKeys = true; explicitNulls = false }
        val reporter = json.decodeFromString(
            CommentEntity.serializer(),
            """{"id":"r","issue_id":"i","team_id":"t","author_id":null,"source":"reporter",
               "audience":"reporter","body":"{\"text\":\"hi\"}","created_at":"$ts","updated_at":"$ts"}""",
        )
        assertEquals(null, reporter.authorId)
        assertTrue(reporter.isFromReporter)
        val legacy = json.decodeFromString(
            CommentEntity.serializer(),
            """{"id":"m","issue_id":"i","team_id":"t","author_id":"u","created_at":"$ts","updated_at":"$ts"}""",
        )
        assertEquals(DomainContract.commentAudienceTeam, legacy.audience)
        assertFalse(legacy.isToReporter)
    }

    // The thread groups a reporter's reply like any other comment: the ONE
    // grouping rule knows nothing about who wrote the row.
    @Test
    fun reporterCommentsThreadLikeAnyOther() {
        val threads = threadComments(
            listOf(
                row("a"),
                row("r", source = DomainContract.commentSourceReporter, authorId = null),
                row("a1", "a"),
            ),
        )
        assertEquals(listOf("a", "r"), threads.topLevel.map { it.id })
        assertEquals(listOf("a1"), threads.repliesByParent["a"]?.map { it.id })
    }
}
