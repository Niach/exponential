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
        startedReason: String? = null,
        resumedFromId: String? = null,
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team-1",
        userId = userId,
        actionName = actionName,
        startedReason = startedReason,
        resumedFromId = resumedFromId,
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

    /** EXP-530: automation-started rows are the device's own doing — a user's
     * pending start watch must never grab one, even when the action name and
     * timing line up. */
    @Test
    fun automationStartedRowsNeverMatch() {
        val key = StartedRunKey.Action("Fix merge conflicts")
        assertFalse(
            matches(
                session(actionName = "Fix merge conflicts", startedReason = "schedule"),
                key,
            ),
        )
        assertFalse(
            matches(
                session(actionName = "Fix merge conflicts", startedReason = "event"),
                key,
            ),
        )
        // Batch/issue watches reject them too — the reason cut runs first.
        assertFalse(matches(session(startedReason = "schedule"), StartedRunKey.Batch))
        assertFalse(
            matches(
                session(issueId = "issue-1", startedReason = "event"),
                StartedRunKey.Issue("issue-1"),
            ),
        )
    }

    @Test
    fun unparseableStartedAtNeverMatches() {
        // Fail CLOSED here (unlike liveness): navigating into the wrong
        // session is worse than not navigating at all.
        assertFalse(matches(session(issueId = "issue-1", startedAt = "nonsense"), StartedRunKey.Issue("issue-1")))
    }

    // ── EXP-637: a resumed run ──────────────────────────────────────────────

    /** The desktop stamps the ENDED run it continues on the new row, so the
     * match is exact — no name or timing guessing. */
    @Test
    fun resumedRunMatchesOnTheRunItContinues() {
        val key = StartedRunKey.Resumed("sess-old")
        assertTrue(matches(session(resumedFromId = "sess-old"), key))
        assertFalse(matches(session(resumedFromId = "sess-other"), key))
        // A fresh run of the same action is not the resume we sent.
        assertFalse(matches(session(actionName = "Refresh screenshots"), key))
    }

    @Test
    fun resumedRunKeepsTheOwnershipTimingAndAutomationCuts() {
        val key = StartedRunKey.Resumed("sess-old")
        assertFalse(matches(session(resumedFromId = "sess-old", userId = "user-2"), key))
        assertFalse(
            matches(
                session(resumedFromId = "sess-old", startedAt = "2026-07-17T11:00:00Z"),
                key,
            ),
        )
        // An automation that happened to resume something is the device's own
        // doing — the reason cut runs before the key.
        assertFalse(
            matches(
                session(resumedFromId = "sess-old", startedReason = "schedule"),
                key,
            ),
        )
    }
}
