package com.exponential.app.domain

import com.exponential.app.data.api.OpenPull
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.TeamEntity

/**
 * One team's `repositories.openPulls` repo: the open pull requests GitHub
 * lists for it, in fetch order (EXP-1244).
 */
data class PullRepo(
    val teamId: String,
    val repositoryId: String,
    val fullName: String,
    val pulls: List<OpenPull>,
)

/**
 * EXP-1244: the Reviews nav entry (bottom-bar tab). [dot] = the queue is
 * non-empty; [shows] = no team, some team not in yolo mode, or the dot is lit.
 */
data class ReviewsNav(val dot: Boolean, val shows: Boolean)

/**
 * EXP-1244: the Reviews queue, ONE pure function ×4 (web
 * `lib/reviews-queue.ts` `reviewsQueue`, desktop `domain::reviews_queue`, iOS
 * `ReviewsQueue.build`), locked by
 * `packages/domain-contract/fixtures/reviews-queue.json` (its `_doc` = the
 * rules) through `ReviewsQueueTest`. Synced issues + runs + the
 * `repositories.openPulls` results → board bands, "Agent runs" bands and the
 * unlinked-PR repository bands.
 */
object ReviewsQueue {

    /** A repository band's trailing caption on a single-team list ×4. */
    const val REPO_BAND_CAPTION = "not linked to an issue"

    /** The "Agent runs" band's trailing caption on a single-team list ×4. */
    const val RUN_BAND_CAPTION = "opened by a coding run"

    /** One open PR: every issue it links, `issues[0]` = the representative. */
    data class Entry(val key: String, val issues: List<IssueEntity>) {
        val representative: IssueEntity get() = issues.first()
    }

    data class BoardGroup(val board: BoardEntity, val entries: List<Entry>)

    data class RunGroup(val teamId: String, val sessions: List<CodingSessionEntity>)

    data class Result(
        val boardGroups: List<BoardGroup>,
        val runGroups: List<RunGroup>,
        val repoGroups: List<PullRepo>,
        val count: Int,
    )

    private const val OPEN = "open"

    private fun present(url: String?): Boolean = !url.isNullOrEmpty()

    /** created_at DESC (as instants), then id ASC. */
    private val issuesNewestFirst: Comparator<IssueEntity> =
        compareByDescending<IssueEntity> { sortableTimestamp(it.createdAt) }.thenBy { it.id }

    private val sessionsNewestFirst: Comparator<CodingSessionEntity> =
        compareByDescending<CodingSessionEntity> { sortableTimestamp(it.createdAt) }.thenBy { it.id }

    /**
     * Rule (6): the run entries over in-scope [sessions] — issue-less, open, a
     * non-empty pr_url no in-scope issue carries ([issueUrls]); the newest row
     * per pr_url, newest first.
     */
    fun runs(sessions: List<CodingSessionEntity>, issueUrls: Set<String>): List<CodingSessionEntity> {
        val byUrl = LinkedHashMap<String, CodingSessionEntity>()
        for (session in sessions) {
            if (session.issueId != null || session.prState != OPEN) continue
            val url = session.prUrl
            if (url.isNullOrEmpty() || url in issueUrls) continue
            val current = byUrl[url]
            if (current == null || sessionsNewestFirst.compare(session, current) < 0) {
                byUrl[url] = session
            }
        }
        return byUrl.values.sortedWith(sessionsNewestFirst)
    }

    /**
     * The nav entry over the SAME queue (fixture `_navDoc` + `navCases`, web
     * `reviewsNav`): [yolo] = each in-scope team's yolo flag, [count] =
     * [build]'s count over the Reviews screen's inputs.
     */
    fun nav(yolo: List<Boolean>, count: Int): ReviewsNav {
        val dot = count > 0
        return ReviewsNav(dot = dot, shows = yolo.isEmpty() || yolo.any { !it } || dot)
    }

    fun build(
        /** In DISPLAY order. */
        teams: List<TeamEntity>,
        boards: List<BoardEntity>,
        issues: List<IssueEntity>,
        sessions: List<CodingSessionEntity>,
        pulls: List<PullRepo>,
    ): Result {
        val teamOrder = HashMap<String, Int>()
        teams.forEachIndexed { index, team -> teamOrder.putIfAbsent(team.id, index) }
        val boardById = boards.filter { it.teamId in teamOrder }.associateBy { it.id }
        // (1) Scope.
        val scopedIssues = issues.filter { it.boardId in boardById }
        val scopedSessions = sessions.filter { it.teamId in teamOrder }

        // (2) One entry per open PR, issues newest first (id ascending on a tie).
        val byKey = LinkedHashMap<String, MutableList<IssueEntity>>()
        for (issue in scopedIssues) {
            if (issue.prState != OPEN) continue
            val key = issue.prUrl?.takeIf { it.isNotEmpty() } ?: "issue:${issue.id}"
            byKey.getOrPut(key) { mutableListOf() }.add(issue)
        }
        // (3) Newest representative first, under the representative's board.
        val entries = byKey
            .map { (key, rows) -> Entry(key, rows.sortedWith(issuesNewestFirst)) }
            .sortedWith { a, b -> issuesNewestFirst.compare(a.representative, b.representative) }
        val entriesByBoard = LinkedHashMap<String, MutableList<Entry>>()
        for (entry in entries) {
            entriesByBoard.getOrPut(entry.representative.boardId) { mutableListOf() }.add(entry)
        }
        // (4) Team order, sort_order (null last), name case-insensitive, id.
        val boardGroups = entriesByBoard
            .map { (boardId, rows) -> BoardGroup(boardById.getValue(boardId), rows) }
            .sortedWith(
                compareBy<BoardGroup> { teamOrder.getValue(it.board.teamId) }
                    .thenBy { it.board.sortOrder }
                    .thenBy { it.board.name.lowercase() }
                    .thenBy { it.board.id },
            )

        // (5) Linked = any in-scope issue's or run's PR, whatever its state.
        val issueUrls = scopedIssues.mapNotNullTo(HashSet()) { it.prUrl?.takeIf(::present) }
        val linked = HashSet(issueUrls).apply {
            scopedSessions.mapNotNullTo(this) { it.prUrl?.takeIf(::present) }
        }

        // (6) A run's OWN PR, banded per team in team order.
        val runs = runs(scopedSessions, issueUrls)
        val runGroups = teams
            .distinctBy { it.id }
            .map { team -> RunGroup(team.id, runs.filter { it.teamId == team.id }) }
            .filter { it.sessions.isNotEmpty() }

        // (7) The unlinked pulls: team order, then fetch order (stable sort).
        val repoGroups = pulls
            .filter { it.teamId in teamOrder }
            .sortedBy { teamOrder.getValue(it.teamId) }
            .map { repo -> repo.copy(pulls = repo.pulls.filter { it.url !in linked }) }
            .filter { it.pulls.isNotEmpty() }

        // (8)
        return Result(
            boardGroups = boardGroups,
            runGroups = runGroups,
            repoGroups = repoGroups,
            count = entries.size + runs.size + repoGroups.sumOf { it.pulls.size },
        )
    }
}
