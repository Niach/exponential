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

    // chats.json: ONE active team drives every phone screen — the Board tab
    // follows a team switch (iOS AppNavigator.applyTeam).
    @Test
    fun currentBoardFollowsTheSelectedTeam() {
        val teams = listOf(team("ws-a"), team("ws-b"))
        val boards = listOf(board("a1", "ws-a"), board("b1", "ws-b"), board("b2", "ws-b"))
        // The remembered board is team A's, but team B is active.
        assertEquals("b1", currentBoardFor(teams, boards, "a1", "ws-b")?.id)
        // The team's own remembered board wins over its first.
        assertEquals("b2", currentBoardFor(teams, boards, "b2", "ws-b")?.id)
    }

    @Test
    fun boardlessSelectedTeamHasNoCurrentBoard() {
        val teams = listOf(team("ws-a"), team("ws-empty"))
        val boards = listOf(board("a1", "ws-a"))
        assertNull(currentBoardFor(teams, boards, "a1", "ws-empty"))
    }

    @Test
    fun unresolvedSelectionKeepsTheLastUsedBoard() {
        val teams = listOf(team("ws-a"), team("ws-b"))
        val boards = listOf(board("a1", "ws-a"), board("b1", "ws-b"))
        assertEquals("b1", currentBoardFor(teams, boards, "b1", null)?.id)
        // A stale id from the previous account (not synced here) too.
        assertEquals("b1", currentBoardFor(teams, boards, "b1", "ws-other-server")?.id)
        assertEquals("a1", currentBoardFor(teams, boards, "gone", null)?.id)
    }

    @Test
    fun teamSwitchRemembersTheTeamsFirstBoard() {
        val boards = listOf(board("a1", "ws-a"), board("b1", "ws-b"), board("b2", "ws-b"))
        assertEquals("b1", boardToRememberOnTeamSwitch(boards, "a1", "ws-b"))
        assertEquals("b1", boardToRememberOnTeamSwitch(boards, null, "ws-b"))
        // Already on one of the team's boards: keep it.
        assertNull(boardToRememberOnTeamSwitch(boards, "b2", "ws-b"))
        // A board-less team remembers nothing.
        assertNull(boardToRememberOnTeamSwitch(boards, "a1", "ws-empty"))
    }
}
