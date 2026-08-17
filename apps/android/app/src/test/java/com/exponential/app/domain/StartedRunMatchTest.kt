package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-536: every start surface navigates into the run it just launched, so
// the row the desktop inserts has to be recognizable from the client. These
// are the rules the six Android surfaces share.
class StartedRunMatchTest {

    // 2026-07-17T12:00:00Z; the watch cuts off at now - SKEW_MS.
    private val nowMs = 1_784_289_600_000L
    private val cutoffMs = nowMs - StartedRunMatch.SKEW_MS

    private fun session(
        id: String = "sess-1",
        issueId: String? = null,
        actionName: String? = null,
        userId: String = "user-1",
        startedAt: String = "2026-07-17T11:59:30Z",
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team-1",
        userId = userId,
        actionName = actionName,
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = startedAt,
    )

    private fun matches(session: CodingSessionEntity, key: StartedRunKey) =
        StartedRunMatch.matches(session, key, "user-1", cutoffMs)

    @Test
    fun oneIssueIsASingleRunTwoOrMoreIsABatch() {
        assertNull(StartedRunKey.forIssues(emptyList()))
        assertEquals(StartedRunKey.Issue("a"), StartedRunKey.forIssues(listOf("a")))
        assertEquals(StartedRunKey.Batch, StartedRunKey.forIssues(listOf("a", "b")))
    }

    @Test
    fun singleRunMatchesItsOwnIssueRow() {
        assertTrue(matches(session(issueId = "issue-1"), StartedRunKey.Issue("issue-1")))
        assertFalse(matches(session(issueId = "issue-2"), StartedRunKey.Issue("issue-1")))
    }

    @Test
    fun batchMatchesTheIssuelessActionlessRow() {
        assertTrue(matches(session(), StartedRunKey.Batch))
        // A single-issue run is not the batch we started…
        assertFalse(matches(session(issueId = "issue-1"), StartedRunKey.Batch))
        // …and neither is an action run, which also carries no issue.
        assertFalse(matches(session(actionName = "Fix merge conflicts"), StartedRunKey.Batch))
    }

    @Test
    fun actionRunMatchesOnTheNameSnapshot() {
        val key = StartedRunKey.Action("Fix merge conflicts")
        // action_id is NULL for the builtins, so the name is the only key.
        assertTrue(matches(session(actionName = "Fix merge conflicts"), key))
        assertFalse(matches(session(actionName = "Create action"), key))
    }

    @Test
    fun anIssueRunIgnoresAnActionRunOnTheSameIssue() {
        val row = session(issueId = "issue-1", actionName = "Fix merge conflicts")
        assertFalse(matches(row, StartedRunKey.Issue("issue-1")))
    }

    @Test
    fun teammatesRunsAndOldRowsNeverMatch() {
        val key = StartedRunKey.Issue("issue-1")
        assertFalse(matches(session(issueId = "issue-1", userId = "user-2"), key))
        // Started an hour ago — a pre-existing session, not the one we sent.
        assertFalse(matches(session(issueId = "issue-1", startedAt = "2026-07-17T11:00:00Z"), key))
    }

    @Test
    fun unparseableStartedAtNeverMatches() {
        // Fail CLOSED here (unlike liveness): navigating into the wrong
        // session is worse than not navigating at all.
        assertFalse(matches(session(issueId = "issue-1", startedAt = "nonsense"), StartedRunKey.Issue("issue-1")))
    }
}
