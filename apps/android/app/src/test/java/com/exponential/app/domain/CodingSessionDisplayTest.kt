package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-358: the `merged` status is the FIRST thing the display state checks —
// a merged-but-alive session reads as "Merged" no matter what the linked issue
// says. The legacy in_review + prState=merged split stays for rows written
// before the flip (and older servers).
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
    fun mergedStatusRendersMerged() {
        assertEquals(
            CodingSessionDisplayState.Merged,
            codingSessionDisplayState(session("merged"), "merged"),
        )
    }

    @Test
    fun mergedStatusWinsOverNeedsInput() {
        // The merge outcome is the headline; a stale attention flag must not
        // repaint the row amber.
        assertEquals(
            CodingSessionDisplayState.Merged,
            codingSessionDisplayState(session("merged", needsInput = true), "open"),
        )
    }

    @Test
    fun legacyInReviewWithMergedPrStaysDone() {
        // Pre-EXP-358 rows (and older servers) never get the `merged` status —
        // they keep splitting on the issue's PR outcome.
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
    fun runningWithNeedsInputIsNeedsInput() {
        assertEquals(
            CodingSessionDisplayState.NeedsInput,
            codingSessionDisplayState(session("running", needsInput = true), null),
        )
    }
}
