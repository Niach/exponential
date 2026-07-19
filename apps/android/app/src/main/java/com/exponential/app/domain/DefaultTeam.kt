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
