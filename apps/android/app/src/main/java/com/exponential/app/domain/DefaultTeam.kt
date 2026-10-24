package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity

/**
 * Default team for the global selection when none is set (EXP-166/EXP-168):
 * the team of the last-opened board (the Issues root's context), else the
 * first team that HAS a board (mirroring AppViewModel.currentBoard's
 * fallback, so Agents/Reviews scope to the same team whose board the
 * Issues root shows), else the first synced team. Null for an EMPTY
 * team list — "Resync now" wipes tables before refetching, and selecting
 * off that transient would pin an arbitrary row (same caveat as iOS
 * AppNavigator's non-empty-emissions rule).
 */
fun defaultTeamId(
    teams: List<TeamEntity>,
    boards: List<BoardEntity>,
    lastBoardId: String?,
): String? {
    if (teams.isEmpty()) return null
    val lastBoardTeam = lastBoardId
        ?.let { last -> boards.firstOrNull { it.id == last }?.teamId }
        ?.let { wsId -> teams.firstOrNull { it.id == wsId }?.id }
    return lastBoardTeam
        ?: teams.firstOrNull { ws -> boards.any { it.teamId == ws.id } }?.id
        ?: teams.first().id
}

/**
 * The Board tab's current board (chats.json: ONE active team drives every
 * phone screen). With a selected team that is synced: the last-used board
 * when it belongs to that team, else that team's first board, else NONE (a
 * board-less team shows the empty board state, never another team's
 * board). Before a team resolves (or while a stale id from the previous
 * account is still selected): the last-used board, else the first team's
 * first board, else any board.
 */
fun currentBoardFor(
    teams: List<TeamEntity>,
    boards: List<BoardEntity>,
    lastBoardId: String?,
    selectedTeamId: String?,
): BoardEntity? {
    val lastUsed = lastBoardId?.let { last -> boards.firstOrNull { it.id == last } }
    if (selectedTeamId != null && teams.any { it.id == selectedTeamId }) {
        return lastUsed?.takeIf { it.teamId == selectedTeamId }
            ?: boards.firstOrNull { it.teamId == selectedTeamId }
    }
    return lastUsed
        ?: teams.firstNotNullOfOrNull { team -> boards.firstOrNull { it.teamId == team.id } }
        ?: boards.firstOrNull()
}

/**
 * The board a team switch remembers as last-used (iOS `applyTeam`): null
 * while the remembered board already belongs to [teamId] (or the team has
 * no board), else the team's first board.
 */
fun boardToRememberOnTeamSwitch(
    boards: List<BoardEntity>,
    lastBoardId: String?,
    teamId: String,
): String? {
    if (boards.any { it.id == lastBoardId && it.teamId == teamId }) return null
    return boards.firstOrNull { it.teamId == teamId }?.id
}
