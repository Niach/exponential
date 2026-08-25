package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-153: the client-side staleness guard must mirror the server sweep's
// rule — a live row (`running` or the EXP-194 `in_review` parking spot) is
// live only while its heartbeat (updated_at) is inside the contract stale
// window; stale and `ended` rows render as absent.
class CodingSessionLivenessTest {

    // 2026-07-17T12:00:00Z
    private val nowMs = 1_784_289_600_000L

    private fun session(status: String, updatedAt: String) = CodingSessionEntity(
        id = "sess-1",
        issueId = "issue-1",
        teamId = "ws-1",
        userId = "user-1",
        status = status,
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = updatedAt,
    )

    @Test
    fun liveWithinStaleWindow() {
        // Heartbeat 30 minutes ago — well inside the 2h window.
        assertTrue(CodingSessionLiveness.isLive(session("running", "2026-07-17T11:30:00Z"), nowMs))
    }

    @Test
    fun stalePastWindow() {
        // Last heartbeat 3h ago — past the window; renders as absent.
        assertFalse(CodingSessionLiveness.isLive(session("running", "2026-07-17T09:00:00Z"), nowMs))
    }

    @Test
    fun nonRunningIsNeverLive() {
        assertFalse(CodingSessionLiveness.isLive(session("ended", "2026-07-17T11:59:00Z"), nowMs))
    }

    @Test
    fun inReviewWithinStaleWindowIsLive() {
        // EXP-194: the `in_review` PR-open parking spot is live while fresh —
        // the terminal is still alive ("ready for review").
        assertTrue(CodingSessionLiveness.isLive(session("in_review", "2026-07-17T11:30:00Z"), nowMs))
    }

    @Test
    fun inReviewPastWindowIsNotLive() {
        // A stale in_review heartbeat drops out just like a stale running one.
        assertFalse(CodingSessionLiveness.isLive(session("in_review", "2026-07-17T09:00:00Z"), nowMs))
    }

    @Test
    fun endedIsNeverLiveEvenWhenFresh() {
        // `ended` is the desktop kill-switch — never live, regardless of a
        // fresh heartbeat.
        assertFalse(CodingSessionLiveness.isLive(session("ended", "2026-07-17T11:59:59Z"), nowMs))
    }

    @Test
    fun unparseableTimestampFailsOpen() {
        // A garbled liveness signal must never hide a session the server
        // still considers alive — the sweep is the backstop.
        assertNull(CodingSessionLiveness.parseEpochMs("not-a-timestamp"))
        assertTrue(CodingSessionLiveness.isLive(session("running", "not-a-timestamp"), nowMs))
    }

    @Test
    fun parsesOffsetForm() {
        // Postgres/serializer offset form (+00:00) parses via the
        // OffsetDateTime fallback.
        assertFalse(CodingSessionLiveness.isStale("2026-07-17T11:30:00+00:00", nowMs))
        assertTrue(CodingSessionLiveness.isStale("2026-07-17T09:00:00+00:00", nowMs))
    }

    @Test
    fun parsesElectricPostgresTextForm() {
        // Electric-synced rows carry Postgres text timestamps (space separator,
        // hour-only offset). Before WireTimestamps these never parsed, which
        // kept the staleness guard permanently fail-open on synced rows.
        assertFalse(CodingSessionLiveness.isStale("2026-07-17 11:30:00.123456+00", nowMs))
        assertTrue(CodingSessionLiveness.isStale("2026-07-17 09:00:00+00", nowMs))
        assertTrue(
            CodingSessionLiveness.isLive(session("running", "2026-07-17 11:30:00+00"), nowMs),
        )
        assertFalse(
            CodingSessionLiveness.isLive(session("running", "2026-07-17 08:00:00+00"), nowMs),
        )
    }
}
