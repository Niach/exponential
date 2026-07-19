package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-166/EXP-168: the app-level selection bootstrap must prefer the team
// of the last-opened board (the Issues root's context) and never select off
// an empty (mid-resync) team list.
class DefaultTeamTest {

    private fun team(id: String) = TeamEntity(
        id = id,
        name = "Team $id",
        slug = id,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    private fun board(id: String, teamId: String) = BoardEntity(
        id = id,
        teamId = teamId,
        name = "Board $id",
        slug = id,
        prefix = id.take(3).uppercase(),
        color = "#8b5cf6",
        sortOrder = 1.0,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    @Test
    fun lastBoardTeamWins() {
        val teams = listOf(team("ws-personal"), team("ws-team"))
        val boards = listOf(board("p1", "ws-personal"), board("p2", "ws-team"))
        assertEquals("ws-team", defaultTeamId(teams, boards, "p2"))
    }

    @Test
    fun danglingLastBoardFallsToFirstTeamWithABoard() {
        // Mirrors AppViewModel.currentBoard: with no usable last-board the
        // Issues root shows the first team's first board — ws-b here, so
        // the selection must scope there too (a boardless personal team
        // must not win just by sorting first).
        val teams = listOf(team("ws-personal"), team("ws-b"))
        val boards = listOf(board("p1", "ws-b"))
        assertEquals("ws-b", defaultTeamId(teams, boards, "p-deleted"))
    }

    @Test
    fun noBoardsAnywhereFallsToFirstTeam() {
        val teams = listOf(team("ws-a"), team("ws-b"))
        assertEquals("ws-a", defaultTeamId(teams, emptyList(), null))
    }

    @Test
    fun lastBoardInUnsyncedTeamFallsToFirst() {
        // The board row points at a team that hasn't synced (yet) — the
        // selection must stay valid against the live team list.
        val teams = listOf(team("ws-a"))
        val boards = listOf(board("p1", "ws-gone"))
        assertEquals("ws-a", defaultTeamId(teams, boards, "p1"))
    }

    @Test
    fun emptyTeamsYieldNull() {
        // "Resync now" wipes tables before refetching — never pin a selection
        // off that transient.
        assertNull(defaultTeamId(emptyList(), emptyList(), "p1"))
    }
}
