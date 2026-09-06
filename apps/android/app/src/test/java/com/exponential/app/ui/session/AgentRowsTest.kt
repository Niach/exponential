package com.exponential.app.ui.session

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.pastRunByline
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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
        actionName: String? = null,
        prUrl: String? = null,
        prNumber: Int? = null,
        prState: String? = null,
        // EXP-746: `schedule`/`event` is what makes a run an AUTOMATED one —
        // the whole predicate behind "Past".
        startedReason: String? = null,
        agent: String? = null,
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = teamId,
        userId = userId,
        status = status,
        branch = branch,
        agent = agent,
        endedBy = endedBy,
        endedAt = endedAt,
        actionName = actionName,
        startedReason = startedReason,
        prUrl = prUrl,
        prNumber = prNumber,
        prState = prState,
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = updatedAt,
    )

    // A FINISHED, person-started run — what "Past" lists (EXP-746).
    private fun pastRun(
        id: String,
        userId: String = "me",
        teamId: String = "team-1",
        issueId: String? = null,
        endedBy: String? = "agent",
        endedAt: String? = "2026-07-17T11:00:00Z",
        updatedAt: String = "2026-07-17T11:00:00Z",
        startedAt: String = "2026-07-17T09:00:00Z",
        startedReason: String? = null,
        agent: String? = "claude",
    ) = session(
        id = id,
        userId = userId,
        issueId = issueId,
        teamId = teamId,
        status = "ended",
        endedBy = endedBy,
        endedAt = endedAt,
        updatedAt = updatedAt,
        startedAt = startedAt,
        startedReason = startedReason,
        agent = agent,
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

    // ── EXP-734: a run's OWN pull request (an action or chat run whose PR
    // links no issue) merges through the SESSION, not through an issue ──────

    @Test
    fun `an action run with its own open PR merges through the session`() {
        val rows = agentRows(
            sessions = listOf(
                session(
                    "action-run",
                    userId = "me",
                    issueId = null,
                    status = "in_review",
                    actionName = "Refresh screenshots",
                    branch = "exp/refresh-shots-1a2b3c4d",
                    prUrl = "https://github.com/o/r/pull/7",
                    prNumber = 7,
                    prState = "open",
                ),
            ),
            issues = emptyList(),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(MergeTarget.Session("action-run"), rows.single().mergeTarget)
        assertEquals("session:action-run", rows.single().mergeTarget?.key)
    }

    @Test
    fun `a chat run with its own open PR merges through the session`() {
        val rows = agentRows(
            sessions = listOf(
                session(
                    "chat-run",
                    userId = "me",
                    issueId = null,
                    branch = "exp/chat-1a2b3c4d",
                    prUrl = "https://github.com/o/r/pull/8",
                    prNumber = 8,
                    prState = "open",
                ),
            ),
            issues = emptyList(),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(MergeTarget.Session("chat-run"), rows.single().mergeTarget)
    }

    @Test
    fun `an issue run keeps merging through its issue`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(
                issue("issue-1", prUrl = "https://github.com/o/r/pull/1", prState = "open"),
            ),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(MergeTarget.Issue("issue-1"), rows.single().mergeTarget)
    }

    @Test
    fun `a batch row still merges through its resolved representative issue`() {
        val rows = agentRows(
            sessions = listOf(
                session(
                    "reviewing",
                    userId = "me",
                    issueId = null,
                    status = "in_review",
                    branch = "exp/batch-abcd1234",
                ),
            ),
            issues = listOf(
                issue("a", prUrl = "https://github.com/o/r/pull/1", prState = "open", branch = "exp/batch-abcd1234"),
            ),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(MergeTarget.Issue("a"), rows.single().mergeTarget)
    }

    @Test
    fun `a run whose own PR is already merged has nothing to merge`() {
        val rows = agentRows(
            sessions = listOf(
                session(
                    "action-run",
                    userId = "me",
                    issueId = null,
                    status = "in_review",
                    actionName = "Refresh screenshots",
                    prUrl = "https://github.com/o/r/pull/7",
                    prNumber = 7,
                    prState = "merged",
                ),
                // No PR at all — the common issueless action run.
                session("no-pr", userId = "me", issueId = null, actionName = "Refresh screenshots"),
                // An issue run whose PR closed unmerged.
                session("issue-run", userId = "me"),
            ),
            issues = listOf(
                issue("issue-1", prUrl = "https://github.com/o/r/pull/2", prState = "closed"),
            ),
            boards = listOf(board("board-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertNull(rows.single { it.session.id == "action-run" }.mergeTarget)
        assertNull(rows.single { it.session.id == "no-pr" }.mergeTarget)
        assertNull(rows.single { it.session.id == "issue-run" }.mergeTarget)
    }

    // ── EXP-746: the Devices screen's "Past" list ───────────────────────────

    @Test
    fun `lists only the caller's own finished runs in this team`() {
        val rows = pastRunRows(
            sessions = listOf(
                pastRun("mine"),
                // EXP-673: a person-started run reports, then ends with its
                // tab (or a kill) — every ended path still lists.
                pastRun("mine-killed", endedBy = "user"),
                pastRun("mine-merged", endedBy = "merge"),
                // Pre-EXP-637 rows carry no ended_by at all.
                pastRun("legacy", endedBy = null),
                pastRun("theirs", userId = "teammate"),
                pastRun("elsewhere", teamId = "team-2"),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        // Equal `ended_at` stamps keep their input order (a stable sort).
        assertEquals(
            listOf("mine", "mine-killed", "mine-merged", "legacy"),
            rows.map { it.session.id },
        )
    }

    @Test
    fun `a still-live run is never a past run`() {
        val rows = pastRunRows(
            sessions = listOf(
                session("running", userId = "me", issueId = null),
                pastRun("done"),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("done"), rows.map { it.session.id })
    }

    @Test
    fun `a scheduled run never lists under Past`() {
        // The DAO already filters on `started_reason IS NULL`, but the pure
        // rule has to hold on its own: an automation-heavy team would
        // otherwise see its Automations rows leak into this list, and the
        // query cap would push the real ones off the end.
        val mixed = (1..60).map { i ->
            pastRun(
                "run-$i",
                endedAt = "2026-07-%02dT11:00:00Z".format((i % 28) + 1),
                startedReason = when (i % 3) {
                    0 -> "schedule"
                    1 -> "event"
                    else -> null
                },
            )
        }
        val rows = pastRunRows(
            sessions = mixed,
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(20, rows.size)
        assertTrue(rows.all { it.session.startedReason == null })
    }

    @Test
    fun `newest first by when the run ended, falling back to its heartbeat`() {
        val rows = pastRunRows(
            sessions = listOf(
                pastRun("middle", endedAt = "2026-07-17T10:00:00Z"),
                pastRun("newest", endedAt = "2026-07-17T11:00:00Z"),
                // Swept before it ever stamped ended_at — orders off
                // updated_at, the heartbeat stamp.
                pastRun("oldest", endedAt = null, updatedAt = "2026-07-17T08:00:00Z"),
            ),
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("newest", "middle", "oldest"), rows.map { it.session.id })
    }

    @Test
    fun `the list is capped at twenty`() {
        val rows = pastRunRows(
            sessions = (1..40).map {
                pastRun("run-$it", endedAt = "2026-07-%02dT11:00:00Z".format(it % 28 + 1))
            },
            issues = emptyList(),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(PAST_RUN_LIMIT, rows.size)
        assertEquals(20, PAST_RUN_LIMIT)
        assertEquals("2026-07-28T11:00:00Z", rows.first().session.endedAt)
    }

    @Test
    fun `signed out or no team selected lists nothing`() {
        val sessions = listOf(pastRun("mine"))
        assertEquals(
            emptyList<PastRunRow>(),
            pastRunRows(sessions, emptyList(), currentUserId = null, teamId = "team-1"),
        )
        assertEquals(
            emptyList<PastRunRow>(),
            pastRunRows(sessions, emptyList(), currentUserId = "me", teamId = null),
        )
    }

    @Test
    fun `an issue-scoped run joins its issue, an action run has none`() {
        val rows = pastRunRows(
            sessions = listOf(
                pastRun("issue-run", issueId = "issue-1", endedAt = "2026-07-17T11:00:00Z"),
                pastRun("action-run", endedAt = "2026-07-17T10:00:00Z"),
            ),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals("EXP-1", rows.first().issue?.identifier)
        assertNull(rows.last().issue)
    }

    @Test
    fun `the past byline names device, agent and who ended it`() {
        // Byte-identical ×4 — web `pastRunByline`, iOS `PastRuns.byline`,
        // desktop `devices_view::past_run_byline`.
        assertEquals(
            "buildbox · Claude Code · ended by you · 5m ago",
            pastRunByline("buildbox", "Claude Code", "user", "5m ago"),
        )
        assertEquals(
            "buildbox · Codex · ended by agent · 2h ago",
            pastRunByline("buildbox", "Codex", "agent", "2h ago"),
        )
        assertEquals(
            "buildbox · ended by the app · 1d ago",
            pastRunByline("buildbox", null, "client", "1d ago"),
        )
        assertEquals(
            "buildbox · ended by a merge · just now",
            pastRunByline("buildbox", "", "merge", "just now"),
        )
        assertEquals(
            "buildbox · ended by the system · just now",
            pastRunByline("buildbox", null, "system", "just now"),
        )
        // An unknown (or absent) ended_by drops its clause rather than
        // printing a raw wire token.
        assertEquals("buildbox · 3m ago", pastRunByline("buildbox", null, null, "3m ago"))
        assertEquals("buildbox · 3m ago", pastRunByline("buildbox", null, "sideways", "3m ago"))
    }
}
