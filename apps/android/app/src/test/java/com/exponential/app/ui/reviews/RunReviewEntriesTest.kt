package com.exponential.app.ui.reviews

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-734: Reviews also lists the pull requests that belong to a RUN rather
// than to an issue — an action or chat run that opened a chore PR via
// `exponential_pr_open({repositoryId, head})`. Nothing links them to a board,
// so they come out of the coding_sessions rows directly.
class RunReviewEntriesTest {

    private fun run(
        id: String,
        prUrl: String? = "https://github.com/o/r/pull/7",
        prNumber: Int? = 7,
        actionName: String? = null,
        branch: String? = "exp/chat-1a2b3c4d",
        startedAt: String = "2026-09-04T10:00:00Z",
    ) = CodingSessionEntity(
        id = id,
        issueId = null,
        teamId = "team-1",
        userId = "me",
        status = "in_review",
        branch = branch,
        actionName = actionName,
        prUrl = prUrl,
        prNumber = prNumber,
        prState = "open",
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = startedAt,
    )

    @Test
    fun `an action run is titled by its action, a chat run reads Chat`() {
        val entries = buildRunEntries(
            listOf(
                run("a", prUrl = "https://github.com/o/r/pull/7", actionName = "Refresh screenshots"),
                run("b", prUrl = "https://github.com/o/r/pull/8", prNumber = 8),
            ),
        )
        assertEquals(listOf("Refresh screenshots", "Chat"), entries.map { it.title })
        assertEquals(listOf("session:a", "session:b"), entries.map { it.groupKey })
        assertEquals(listOf(7, 8), entries.map { it.prNumber })
    }

    @Test
    fun `runs sharing one PR collapse to the newest row`() {
        // A resumed run continues the SAME pull request — one entry, not two.
        val entries = buildRunEntries(
            listOf(
                run("older", startedAt = "2026-09-04T09:00:00Z"),
                run("newer", startedAt = "2026-09-04T11:00:00Z"),
            ),
        )
        assertEquals(listOf("session:newer"), entries.map { it.groupKey })
    }

    @Test
    fun `entries come out newest first`() {
        val entries = buildRunEntries(
            listOf(
                run("old", prUrl = "https://github.com/o/r/pull/1", startedAt = "2026-09-04T08:00:00Z"),
                run("new", prUrl = "https://github.com/o/r/pull/3", startedAt = "2026-09-04T12:00:00Z"),
                run("mid", prUrl = "https://github.com/o/r/pull/2", startedAt = "2026-09-04T10:00:00Z"),
            ),
        )
        assertEquals(listOf("session:new", "session:mid", "session:old"), entries.map { it.groupKey })
    }

    /** A row with no url is nothing to review — the DAO filters on pr_state,
     *  which normally implies a url, so this is the defensive arm. */
    @Test
    fun `a run without a pr url is dropped`() {
        assertTrue(buildRunEntries(listOf(run("a", prUrl = null), run("b", prUrl = ""))).isEmpty())
    }

    /** Postgres timestamps arrive space-separated from Electric; the ordering
     *  must not depend on which form a row carries. */
    @Test
    fun `mixed wire timestamp forms still order correctly`() {
        val entries = buildRunEntries(
            listOf(
                run("space", prUrl = "https://github.com/o/r/pull/1", startedAt = "2026-09-04 08:00:00+00"),
                run("iso", prUrl = "https://github.com/o/r/pull/2", startedAt = "2026-09-04T12:00:00Z"),
            ),
        )
        assertEquals(listOf("session:iso", "session:space"), entries.map { it.groupKey })
    }
}
