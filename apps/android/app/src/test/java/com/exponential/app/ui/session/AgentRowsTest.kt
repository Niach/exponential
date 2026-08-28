package com.exponential.app.ui.session

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-312 follow-up: the Agents list is OWNER-ONLY. A teammate's live session
// cannot be viewed or steered, so listing it only read as "computer not
// online" — it stays synced (issue-detail badges, Reviews) but never rows here.
// It is also SELECTED-TEAM-only, matching web's use-agents-data: an own run in
// another team belongs under that team, not under whichever one is open.
class AgentRowsTest {

    // 2026-07-17T12:00:00Z
    private val nowMs = 1_784_289_600_000L

    private fun session(
        id: String,
        userId: String,
        issueId: String? = "issue-1",
        teamId: String = "team-1",
        status: String = "running",
        branch: String? = null,
        updatedAt: String = "2026-07-17T11:30:00Z",
        endedBy: String? = null,
        endedAt: String? = null,
        startedAt: String = "2026-07-17T09:00:00Z",
        outcome: String? = null,
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = teamId,
        userId = userId,
        status = status,
        branch = branch,
        endedBy = endedBy,
        endedAt = endedAt,
        outcome = outcome,
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = updatedAt,
    )

    // An ENDED run carrying the agent's close-out — what "Recent runs" lists.
    private fun endedRun(
        id: String,
        userId: String = "me",
        teamId: String = "team-1",
        issueId: String? = null,
        endedBy: String? = "agent",
        endedAt: String? = "2026-07-17T11:00:00Z",
        startedAt: String = "2026-07-17T09:00:00Z",
        outcome: String? = "done",
    ) = session(
        id = id,
        userId = userId,
        issueId = issueId,
        teamId = teamId,
        status = "ended",
        endedBy = endedBy,
        endedAt = endedAt,
        startedAt = startedAt,
        outcome = outcome,
    )

    private fun issue(
        id: String,
        boardId: String = "board-1",
        prUrl: String? = null,
        prState: String? = null,
        branch: String? = null,
        createdAt: String = "2026-07-17T09:00:00Z",
    ) = IssueEntity(
        id = id,
        boardId = boardId,
        number = 1,
        identifier = "EXP-1",
        title = "An issue",
        status = "in_progress",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        prState = prState,
        branch = branch,
        createdAt = createdAt,
        updatedAt = "2026-07-17T09:00:00Z",
    )

    private fun board(
        id: String,
        teamId: String = "team-1",
        deletedAt: String? = null,
    ) = BoardEntity(
        id = id,
        teamId = teamId,
        name = "Board",
        slug = id,
        prefix = "EXP",
        color = "#888888",
        sortOrder = 1.0,
        deletedAt = deletedAt,
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T09:00:00Z",
    )

    @Test
    fun `lists only the signed-in user's own sessions`() {
        val rows = agentRows(
            sessions = listOf(
                session("mine", userId = "me"),
                session("theirs", userId = "teammate"),
            ),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("mine"), rows.map { it.session.id })
    }

    @Test
    fun `empty when every live session belongs to someone else`() {
        val rows = agentRows(
            sessions = listOf(
                session("theirs", userId = "teammate"),
                session("also-theirs", userId = "other"),
            ),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `signed out lists nothing`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = null,
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own session in another team is not listed`() {
        val rows = agentRows(
            sessions = listOf(
                session("here", userId = "me"),
                session("elsewhere", userId = "me", teamId = "team-2"),
            ),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("here"), rows.map { it.session.id })
    }

    @Test
    fun `no selected team lists nothing`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = null,
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own stale session still drops out`() {
        // The EXP-153 staleness cut applies on top of the ownership filter.
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me", updatedAt = "2026-07-17T09:00:00Z")),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own batch session lists without an issue link`() {
        // Batch rows carry no issue but DO carry an explicit team_id, so the
        // team scoping keeps them.
        val rows = agentRows(
            sessions = listOf(session("batch", userId = "me", issueId = null)),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(1, rows.size)
        assertNull(rows.single().issue)
    }

    @Test
    fun `own session joins its issue`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            boards = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals("EXP-1", rows.single().issue?.identifier)
    }

    // ── EXP-535: the batch merge shortcut's client-side PR resolution ───────

    @Test
    fun `batch in-review row carries the resolved batch PR, a running one does not`() {
        val rows = agentRows(
            sessions = listOf(
                session("reviewing", userId = "me", issueId = null, status = "in_review", branch = "exp/batch-abcd1234"),
                session("running", userId = "me", issueId = null),
            ),
            issues = listOf(
                issue("a", prUrl = "https://github.com/o/r/pull/1", prState = "open", branch = "exp/batch-abcd1234"),
            ),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals("a", rows.single { it.session.id == "reviewing" }.batchPrIssue?.id)
        assertNull(rows.single { it.session.id == "running" }.batchPrIssue)
    }

    @Test
    fun `resolves the single open batch PR to its newest linked issue`() {
        // Two issues share ONE batch PR (the batch launcher links them all to
        // the same prUrl) — still one distinct PR, newest createdAt wins.
        val reps = openBatchPrRepresentatives(
            issues = listOf(
                issue(
                    "older",
                    prUrl = "https://github.com/o/r/pull/1",
                    prState = "open",
                    branch = "exp/batch-abcd1234",
                    createdAt = "2026-07-17T09:00:00Z",
                ),
                issue(
                    "newer",
                    prUrl = "https://github.com/o/r/pull/1",
                    prState = "open",
                    branch = "exp/batch-abcd1234",
                    createdAt = "2026-07-17T10:00:00Z",
                ),
            ),
            boards = listOf(board("board-1")),
            teamId = "team-1",
        )
        assertEquals("newer", resolveBatchPrIssue(reps, "exp/batch-abcd1234")?.id)
        // EXP-546: a branchless row resolves nothing, even with a single open
        // batch PR to point at.
        assertNull(resolveBatchPrIssue(reps, null))
    }

    @Test
    fun `session branch picks its own PR among concurrent batch runs`() {
        // EXP-545: with the stamped branch a session resolves ITS OWN PR even
        // while a second batch PR is open; a branchless row resolves nothing
        // (EXP-546).
        val reps = openBatchPrRepresentatives(
            issues = listOf(
                issue("a", prUrl = "https://github.com/o/r/pull/1", prState = "open", branch = "exp/batch-abcd1234"),
                issue("b", prUrl = "https://github.com/o/r/pull/2", prState = "open", branch = "exp/batch-ef567890"),
            ),
            boards = listOf(board("board-1")),
            teamId = "team-1",
        )
        assertEquals("a", resolveBatchPrIssue(reps, "exp/batch-abcd1234")?.id)
        assertNull(resolveBatchPrIssue(reps, null))
    }

    @Test
    fun `session whose own PR closed never offers a teammate's PR`() {
        // EXP-545 regression: my batch PR closed unmerged (my session stays
        // in_review — only merge ends it) while a teammate's batch PR is the
        // sole open one. My stamped branch matches nothing open → no Merge.
        val reps = openBatchPrRepresentatives(
            issues = listOf(
                issue("mine", prUrl = "https://github.com/o/r/pull/1", prState = "closed", branch = "exp/batch-abcd1234"),
                issue("theirs", prUrl = "https://github.com/o/r/pull/2", prState = "open", branch = "exp/batch-ef567890"),
            ),
            boards = listOf(board("board-1")),
            teamId = "team-1",
        )
        assertNull(resolveBatchPrIssue(reps, "exp/batch-abcd1234"))
    }

    @Test
    fun `single-issue and non-open PRs never resolve`() {
        // A plain `exp/EXP-12` branch is not a batch PR, a merged batch PR is
        // no longer mergeable, and an issue without a prUrl has no PR at all.
        val reps = openBatchPrRepresentatives(
            issues = listOf(
                issue("single", prUrl = "https://github.com/o/r/pull/1", prState = "open", branch = "exp/EXP-12"),
                issue("merged", prUrl = "https://github.com/o/r/pull/2", prState = "merged", branch = "exp/batch-abcd1234"),
                issue("no-pr", prUrl = null, prState = null, branch = "exp/batch-ef567890"),
            ),
            boards = listOf(board("board-1")),
            teamId = "team-1",
        )
        assertNull(resolveBatchPrIssue(reps, "exp/batch-abcd1234"))
    }

    @Test
    fun `another team's batch PR is out of scope`() {
        // Issues don't sync team_id — the scoping goes through boards, and a
        // trashed board's issues are out too.
        val reps = openBatchPrRepresentatives(
            issues = listOf(
                issue(
                    "elsewhere",
                    boardId = "board-2",
                    prUrl = "https://github.com/o/r/pull/1",
                    prState = "open",
                    branch = "exp/batch-abcd1234",
                ),
            ),
            boards = listOf(board("board-1"), board("board-2", teamId = "team-2")),
            teamId = "team-1",
        )
        assertNull(resolveBatchPrIssue(reps, "exp/batch-abcd1234"))
    }

    // ── EXP-637: the "Recent runs" list ────────────────────────────────────

    @Test
    fun `lists only the caller's own reported runs in this team`() {
        val rows = recentRunRows(
            sessions = listOf(
                endedRun("mine"),
                // EXP-673: a person-started run reports, then ends with its
                // tab (or a kill) — the report still lists.
                endedRun("mine-closed-later", endedBy = "client"),
                endedRun("theirs", userId = "teammate"),
                endedRun("elsewhere", teamId = "team-2"),
                // Killed, merged or swept WITHOUT a report the agent wrote.
                endedRun("killed", endedBy = "user", outcome = null),
                endedRun("merged", endedBy = "merge", outcome = null),
                // Pre-EXP-637 rows carry neither.
                endedRun("legacy", endedBy = null, outcome = null),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
        )
        assertEquals(listOf("mine", "mine-closed-later"), rows.map { it.session.id })
    }

    @Test
    fun `a still-live run is never a recent run`() {
        val rows = recentRunRows(
            sessions = listOf(
                // EXP-673: a person-started run that has reported but is
                // still open for replies.
                session("running", userId = "me", outcome = "done"),
                endedRun("done"),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
        )
        assertEquals(listOf("done"), rows.map { it.session.id })
    }

    @Test
    fun `newest first by when the run ended, falling back to its start`() {
        val rows = recentRunRows(
            sessions = listOf(
                endedRun("middle", endedAt = "2026-07-17T10:00:00Z"),
                endedRun("newest", endedAt = "2026-07-17T11:00:00Z"),
                // Swept before it ever stamped ended_at — orders off its start.
                endedRun("oldest", endedAt = null, startedAt = "2026-07-17T08:00:00Z"),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
        )
        assertEquals(listOf("newest", "middle", "oldest"), rows.map { it.session.id })
    }

    @Test
    fun `caps the list, keeping the newest`() {
        val rows = recentRunRows(
            sessions = (1..25).map {
                endedRun("run-$it", endedAt = "2026-07-17T%02d:00:00Z".format(it % 24))
            },
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            limit = 3,
        )
        assertEquals(3, rows.size)
        assertEquals("2026-07-17T23:00:00Z", rows.first().session.endedAt)
    }

    @Test
    fun `signed out or no team selected lists nothing`() {
        val sessions = listOf(endedRun("mine"))
        assertEquals(
            emptyList<RecentRunRow>(),
            recentRunRows(sessions, emptyList(), currentUserId = null, teamId = "team-1"),
        )
        assertEquals(
            emptyList<RecentRunRow>(),
            recentRunRows(sessions, emptyList(), currentUserId = "me", teamId = null),
        )
    }

    @Test
    fun `an issue-scoped run joins its issue, an action run has none`() {
        val rows = recentRunRows(
            sessions = listOf(
                endedRun("issue-run", issueId = "issue-1", endedAt = "2026-07-17T11:00:00Z"),
                endedRun("action-run", endedAt = "2026-07-17T10:00:00Z"),
            ),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
        )
        assertEquals("EXP-1", rows.first().issue?.identifier)
        assertNull(rows.last().issue)
    }
}
