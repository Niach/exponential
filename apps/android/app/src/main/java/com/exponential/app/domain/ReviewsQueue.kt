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

    /**
     * EXP-1248: one display item of a board band (rule 9, `_groupDoc`).
     * [Pr] = a lone PR (depth 0) or a member of a PR TREE, pre-order under
     * its root; [Stack] = a linear stack, its entries TOP first, then the
     * base-branch row.
     */
    sealed class Item {
        data class Pr(val entry: Entry, val depth: Int) : Item()
        data class Stack(val entries: List<Entry>, val baseBranch: String?) : Item()
    }

    data class BoardGroup(
        val board: BoardEntity,
        /** Every entry, flat, newest first (rule 3). */
        val entries: List<Entry>,
        /** The same entries as the band draws them (rule 9). */
        val items: List<Item> = ReviewsQueue.items(entries),
    )

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

    private fun edge(branch: String?): String? = branch?.takeIf { it.isNotEmpty() }

    /**
     * (9) A band's entries as items, x4 (web `queueItems`, desktop
     * `queue_items`, iOS `ReviewsQueue.items`). Edge: an entry sits on the
     * entry (same band) whose representative's `branch` is its
     * representative's `prBaseBranch`. A component lists where its NEWEST
     * entry would, walked from its ROOT (a cycle breaks where the climb first
     * repeats). Any entry with two children = a TREE: pre-order, children in
     * band order, depth = distance from the root. Otherwise 2+ entries = a
     * STACK item, top first, `baseBranch` = the root's `prBaseBranch`.
     */
    fun items(entries: List<Entry>): List<Item> {
        val owner = HashMap<String, Entry>()
        for (entry in entries) {
            val branch = edge(entry.representative.branch) ?: continue
            if (!owner.containsKey(branch)) owner[branch] = entry
        }
        val parentOf = HashMap<String, Entry>()
        val children = HashMap<String, MutableList<Entry>>()
        for (entry in entries) {
            val parent = edge(entry.representative.prBaseBranch)?.let { owner[it] } ?: continue
            if (parent.key == entry.key) continue
            parentOf[entry.key] = parent
            children.getOrPut(parent.key) { mutableListOf() }.add(entry)
        }
        val placed = HashSet<String>()
        val items = ArrayList<Item>()
        for (start in entries) {
            if (start.key in placed) continue
            // Climb to the root; a cycle stops where it first repeats.
            var root = start
            val climbed = hashSetOf(root.key)
            while (true) {
                val parent = parentOf[root.key] ?: break
                if (parent.key in climbed || parent.key in placed) break
                climbed.add(parent.key)
                root = parent
            }
            // The component under the root, pre-order, children in band order.
            val members = ArrayList<Pair<Entry, Int>>()
            var fork = false
            fun visit(entry: Entry, depth: Int) {
                if (!placed.add(entry.key)) return
                members.add(entry to depth)
                val below = children[entry.key].orEmpty().filter { it.key !in placed }
                if (below.size > 1) fork = true
                for (child in below) visit(child, depth + 1)
            }
            visit(root, 0)
            if (members.size > 1 && !fork) {
                items.add(Item.Stack(members.map { it.first }.reversed(), edge(root.representative.prBaseBranch)))
            } else {
                members.forEach { (entry, depth) -> items.add(Item.Pr(entry, depth)) }
            }
        }
        return items
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
