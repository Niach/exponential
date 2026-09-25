package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1075: the board switcher's other-team dot. Session lists on the phone are
// selected-team scoped, so an own run in ANOTHER team is invisible until you
// switch — this is the pointer back to it. Same own + live predicate as
// AppViewModel.agentsRunning, minus the selected-team filter.
class TeamLiveRunsTest {

    // 2026-07-17T12:00:00Z
    private val nowMs = 1_784_289_600_000L

    private fun session(
        id: String,
        userId: String = "me",
        teamId: String = "team-1",
        status: String = "running",
        needsInput: Boolean = false,
        // Half an hour old — well inside the 2h staleness window.
        updatedAt: String = "2026-07-17T11:30:00Z",
        endedBy: String? = null,
        endedAt: String? = null,
    ) = CodingSessionEntity(
        id = id,
        issueId = "issue-$id",
        teamId = teamId,
        userId = userId,
        status = status,
        needsInput = needsInput,
        endedBy = endedBy,
        endedAt = endedAt,
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = updatedAt,
    )

    @Test
    fun `groups own live runs by team`() {
        val byTeam = liveRunsByTeam(
            listOf(
                session("a", teamId = "team-1"),
                session("b", teamId = "team-2"),
                session("c", teamId = "team-2"),
            ),
            me = "me",
            now = nowMs,
        )
        assertEquals(setOf("team-1", "team-2"), byTeam.keys)
        assertEquals(1, byTeam["team-1"]?.count)
        assertEquals(2, byTeam["team-2"]?.count)
        assertFalse(byTeam["team-2"]?.needsInput ?: true)
    }

    @Test
    fun `a teammate's live run is ignored`() {
        val byTeam = liveRunsByTeam(
            listOf(session("a", userId = "someone-else", teamId = "team-2")),
            me = "me",
            now = nowMs,
        )
        assertTrue(byTeam.isEmpty())
        assertFalse(otherTeamsLive(byTeam, "team-1").any)
    }

    @Test
    fun `no viewer means no dot`() {
        assertTrue(liveRunsByTeam(listOf(session("a")), me = null, now = nowMs).isEmpty())
    }

    @Test
    fun `ended and heartbeat-stale rows drop out`() {
        val byTeam = liveRunsByTeam(
            listOf(
                session(
                    "ended",
                    teamId = "team-2",
                    status = "ended",
                    endedBy = "user",
                    endedAt = "2026-07-17T11:00:00Z",
                ),
                // Running, but the desktop stopped heartbeating three hours ago
                // (EXP-153) — renders as absent, never as a phantom dot.
                session("stale", teamId = "team-3", updatedAt = "2026-07-17T09:00:00Z"),
            ),
            me = "me",
            now = nowMs,
        )
        assertTrue(byTeam.isEmpty())
    }

    @Test
    fun `in_review counts as live`() {
        val byTeam = liveRunsByTeam(
            listOf(session("a", teamId = "team-2", status = "in_review")),
            me = "me",
            now = nowMs,
        )
        assertEquals(1, byTeam["team-2"]?.count)
        // EXP-679: in_review masks needs_input, so the parked PR stays green.
        assertFalse(byTeam["team-2"]?.needsInput ?: true)
    }

    @Test
    fun `needsInput lifts only its own team`() {
        val byTeam = liveRunsByTeam(
            listOf(
                session("a", teamId = "team-2", needsInput = true),
                session("b", teamId = "team-2"),
                session("c", teamId = "team-3"),
            ),
            me = "me",
            now = nowMs,
        )
        assertTrue(byTeam["team-2"]?.needsInput ?: false)
        assertFalse(byTeam["team-3"]?.needsInput ?: true)
        assertNull(byTeam["team-1"])
    }

    @Test
    fun `the selected team is excluded from the dot`() {
        val byTeam = liveRunsByTeam(
            listOf(session("a", teamId = "team-1", needsInput = true)),
            me = "me",
            now = nowMs,
        )
        // Its runs already light the Agents tab — no second dot for them.
        assertFalse(otherTeamsLive(byTeam, "team-1").any)
        assertTrue(otherTeamsLive(byTeam, "team-9").any)
        assertTrue(otherTeamsLive(byTeam, "team-9").needsInput)
        // No team selected yet (first run): every live team still counts.
        assertTrue(otherTeamsLive(byTeam, null).any)
    }

    @Test
    fun `an unselected team's needsInput sets the tone, the selected team's does not`() {
        val byTeam = liveRunsByTeam(
            listOf(
                session("a", teamId = "team-1", needsInput = true),
                session("b", teamId = "team-2"),
            ),
            me = "me",
            now = nowMs,
        )
        val other = otherTeamsLive(byTeam, "team-1")
        assertTrue(other.any)
        assertFalse(other.needsInput)
    }

    @Test
    fun `no other team live means no dot`() {
        assertFalse(otherTeamsLive(emptyMap(), "team-1").any)
    }
}
