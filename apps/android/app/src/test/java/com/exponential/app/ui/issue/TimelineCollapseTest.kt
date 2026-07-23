package com.exponential.app.ui.issue

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-240: runs of >2 consecutive events fold behind a "Show N" expander keyed
// by the run's FIRST event id (stable across sync re-emits); the created item
// and comments never collapse and always break runs.
class TimelineCollapseTest {

    private val ts = "2026-07-01 10:00:00+00"

    private fun event(id: String) = TimelineItem.Event(
        IssueEventEntity(
            id = id,
            issueId = "issue-1",
            teamId = "team-1",
            actorUserId = "actor-1",
            type = "status_changed",
            payload = null,
            createdAt = ts,
            updatedAt = ts,
        ),
    )

    private fun comment(id: String) = TimelineItem.Comment(
        CommentEntity(
            id = id,
            issueId = "issue-1",
            teamId = "team-1",
            authorId = "author-1",
            body = "hi",
            createdAt = ts,
            updatedAt = ts,
        ),
    )

    private fun created() = TimelineItem.Created(
        IssueEntity(
            id = "issue-1",
            boardId = "board-1",
            number = 1,
            identifier = "EXP-1",
            title = "Title",
            status = "todo",
            priority = "none",
            sortOrder = 1.0,
            createdAt = ts,
            updatedAt = ts,
        ),
    )

    @Test
    fun runOfThreeCollapses() {
        val rows = collapseTimeline(
            listOf(event("e1"), event("e2"), event("e3")),
            expandedRuns = emptySet(),
        )
        assertEquals(listOf<TimelineRow>(TimelineRow.CollapsedRun("e1", 3)), rows)
    }

    @Test
    fun runOfTwoStaysExpanded() {
        val rows = collapseTimeline(
            listOf(event("e1"), event("e2")),
            expandedRuns = emptySet(),
        )
        assertEquals(2, rows.size)
        assertTrue(rows.all { it is TimelineRow.Single })
    }

    @Test
    fun expandedRunKeyUnfoldsTheRun() {
        val rows = collapseTimeline(
            listOf(event("e1"), event("e2"), event("e3")),
            expandedRuns = setOf("e1"),
        )
        assertEquals(3, rows.size)
        assertTrue(rows.all { it is TimelineRow.Single })
    }

    @Test
    fun commentsBreakRuns() {
        val rows = collapseTimeline(
            listOf(event("e1"), event("e2"), comment("c1"), event("e3"), event("e4")),
            expandedRuns = emptySet(),
        )
        // Neither event pair reaches the >2 threshold, so nothing collapses.
        assertEquals(5, rows.size)
        assertTrue(rows.all { it is TimelineRow.Single })
    }

    @Test
    fun createdItemNeverCollapsesAndBreaksNoEventRun() {
        val rows = collapseTimeline(
            listOf(created(), event("e1"), event("e2"), event("e3")),
            expandedRuns = emptySet(),
        )
        assertEquals(2, rows.size)
        assertTrue(rows[0] is TimelineRow.Single)
        assertTrue((rows[0] as TimelineRow.Single).item is TimelineItem.Created)
        assertEquals(TimelineRow.CollapsedRun("e1", 3), rows[1])
    }

    @Test
    fun twoIndependentRunsCollapseSeparately() {
        val rows = collapseTimeline(
            listOf(
                event("a1"), event("a2"), event("a3"),
                comment("c1"),
                event("b1"), event("b2"), event("b3"), event("b4"),
            ),
            expandedRuns = setOf("a1"),
        )
        // First run expanded (3 singles), the comment, second run folded as one.
        assertEquals(5, rows.size)
        assertEquals(TimelineRow.CollapsedRun("b1", 4), rows[4])
    }
}
