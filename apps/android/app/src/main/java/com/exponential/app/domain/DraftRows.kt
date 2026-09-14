package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueDraftEntity
import com.exponential.app.data.db.TeamEntity

/**
 * One row of the "Drafts" section (EXP-878) — a synced draft joined to the
 * board it will be filed on. The issue_drafts shape is static per user and NOT
 * team/trash scoped (the twin of pins), so this join is where a draft on an
 * unresolvable board (a trashed or archived board, a team the user left) drops
 * out: no board, no row — never a placeholder.
 *
 * [status] is the draft's resolved status, which is the team's Backlog builtin
 * whenever `status_id` is null (the server's own default at create time).
 */
data class DraftRow(
    val draft: IssueDraftEntity,
    val board: BoardEntity,
    val status: ResolvedIssueStatus,
    /** The board's team, when its row has synced — the list needs only its name. */
    val team: TeamEntity? = null,
)

/**
 * Resolve [drafts] against the synced boards, newest-edited first.
 *
 * [statusesByTeam] is each team's resolved status vocabulary (an absent or
 * empty entry degrades to the constructed builtins, exactly like every other
 * status render path), and [teamsById] is optional display data.
 */
fun resolveDraftRows(
    drafts: List<IssueDraftEntity>,
    boards: List<BoardEntity>,
    statusesByTeam: Map<String, List<ResolvedIssueStatus>>,
    teamsById: Map<String, TeamEntity> = emptyMap(),
): List<DraftRow> {
    if (drafts.isEmpty()) return emptyList()
    val boardsById = boards.associateBy { it.id }
    return drafts
        .mapNotNull { draft ->
            val board = boardsById[draft.boardId] ?: return@mapNotNull null
            val team = statusesByTeam[board.teamId]
                ?.takeIf { it.isNotEmpty() }
                ?: IssueStatusResolver.builtinDefaults
            DraftRow(
                draft = draft,
                board = board,
                // A null status_id IS the team's Backlog builtin (server
                // default), which is exactly what resolve() falls back to.
                status = IssueStatusResolver.resolve(draft.statusId, null, team),
                team = teamsById[board.teamId],
            )
        }
        // Most recently touched first — the draft you just left is the one you
        // come back to. The DAO already orders this way; re-stating it here
        // keeps the pure function self-contained (and testable).
        .sortedByDescending { it.draft.updatedAt }
}
