package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-540: there is no `merged` session status — merging ends the session
// (EXP-498). The in_review + prState=merged split is the old-server tolerance:
// a lagging server can still park a row on in_review with a merged PR.
class CodingSessionDisplayTest {

    private fun session(status: String, needsInput: Boolean = false) = CodingSessionEntity(
        id = "sess-1",
        issueId = "issue-1",
        teamId = "ws-1",
        userId = "user-1",
        status = status,
        needsInput = needsInput,
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T11:30:00Z",
    )

    @Test
    fun inReviewWithMergedPrStaysDone() {
        // Old-server tolerance: a row parked on in_review whose PR is already
        // merged reads as Done, not "ready for review".
        assertEquals(
            CodingSessionDisplayState.Done,
            codingSessionDisplayState(session("in_review"), "merged"),
        )
    }

    @Test
    fun inReviewWithOpenPrIsReview() {
        assertEquals(
            CodingSessionDisplayState.Review,
            codingSessionDisplayState(session("in_review"), "open"),
        )
    }

    @Test
    fun inReviewWithNeedsInputStaysReview() {
        // EXP-531: once the PR is open the session is in review — claude's
        // idle notification (which trips needs_input after the final turn
        // ends) must not mask "Ready for review".
        assertEquals(
            CodingSessionDisplayState.Review,
            codingSessionDisplayState(session("in_review", needsInput = true), "open"),
        )
    }

    @Test
    fun runningWithNeedsInputIsNeedsInput() {
        assertEquals(
            CodingSessionDisplayState.NeedsInput,
            codingSessionDisplayState(session("running", needsInput = true), null),
        )
    }
}
