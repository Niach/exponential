package com.exponential.app.ui.inbox

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.NotificationEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.DomainContract
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Inbox grouping: issue-anchored notifications group per issue — a widget
 * reporter's reply (`reporter_reply`, SLOP-4) included, it is issue-scoped
 * like `issue_comment` — interleaved with the issue-less agent-message and
 * blocked-run rows into the one stream by latest activity (web/iOS/desktop
 * parity), and totalUnread counts them all. Issue-less rows of any other
 * type are dropped.
 */
class InboxGroupingTest {

    private val ts = "2026-07-19 00:00:00+00"

    private fun notification(
        id: String,
        issueId: String? = null,
        teamId: String? = null,
        sessionId: String? = null,
        type: String = DomainContract.notificationTypeIssueComment,
        title: String = "title-$id",
        body: String? = null,
        readAt: String? = null,
    ) = NotificationEntity(
        id = id, userId = "u1", issueId = issueId, teamId = teamId, sessionId = sessionId,
        type = type, title = title, body = body, readAt = readAt, createdAt = ts, updatedAt = ts,
    )

    private fun issue(id: String) = IssueEntity(
        id = id, boardId = "b1", number = 1, identifier = "EXP-1", title = "Issue $id",
        status = "backlog", priority = "none", creatorId = "u1", sortOrder = 1.0,
        createdAt = ts, updatedAt = ts,
    )

    private fun team(id: String, name: String) = TeamEntity(
        id = id, name = name, slug = id, createdAt = ts, updatedAt = ts,
    )

    private val InboxState.issueGroups: List<InboxGroup>
        get() = entries.filterIsInstance<InboxEntry.Issue>().map { it.group }

    /**
     * SLOP-4: a reporter's reply rides its issue's group like any comment —
     * the newest row drives the sentence, and the row opens the issue (not a
     * Results face, not a Support surface).
     */
    @Test
    fun reporterRepliesGroupUnderTheirIssueAndCountTowardTotalUnread() {
        val state = buildInboxState(
            notifications = listOf(
                // Newest-first, like the DAO delivers.
                notification(
                    "n1", issueId = "i1", type = DomainContract.notificationTypeReporterReply,
                    title = "Emma Fischer replied on EXP-1", body = "Still broken on my end.",
                ),
                notification("n2", issueId = "i1"),
                notification("n3", issueId = "i2", type = DomainContract.notificationTypeReporterReply, readAt = ts),
            ),
            issues = listOf(issue("i1"), issue("i2")),
            teams = listOf(team("t1", "Acme")),
        )

        assertEquals(listOf("issue:i1", "issue:i2"), state.entries.map { it.key })
        val first = state.issueGroups[0]
        assertEquals(2, first.notifications.size)
        assertEquals(2, first.unread)
        assertEquals(DomainContract.notificationTypeReporterReply, first.latest.type)
        assertEquals("Emma Fischer replied on EXP-1", first.latest.title)
        assertEquals(false, first.opensResults)
        assertEquals(0, state.issueGroups[1].unread)
        assertEquals(2, state.totalUnread)
    }

    /**
     * EXP-801: an agent's message is its own entry — one per row, never
     * bundled — carrying the resolved team name, and counts toward the total.
     */
    @Test
    fun agentMessagesAreOneEntryEachAndCountTowardTotalUnread() {
        val state = buildInboxState(
            notifications = listOf(
                notification(
                    "m1", teamId = "t1", type = DomainContract.notificationTypeAgentMessage,
                    title = "Ada's agent: Build finished", body = "All green.",
                ),
                notification("m2", teamId = "ghost", type = DomainContract.notificationTypeAgentMessage, readAt = ts),
            ),
            issues = emptyList(),
            teams = listOf(team("t1", "Acme")),
        )

        val messages = state.entries.filterIsInstance<InboxEntry.Message>()
        assertEquals(2, messages.size)
        assertEquals("message:m1", messages[0].key)
        assertEquals("Acme", messages[0].teamName)
        assertEquals("Ada's agent: Build finished", messages[0].notification.title)
        assertEquals(1, messages[0].unread)
        assertNull(messages[1].teamName)
        assertEquals(0, messages[1].unread)
        assertEquals(1, state.totalUnread)
    }

    /**
     * EXP-933: an agent message naming an issue groups under that issue, and
     * the row opens the issue's Results face while it is the latest.
     */
    @Test
    fun agentMessageWithAnIssueOpensThatIssuesResults() {
        val state = buildInboxState(
            notifications = listOf(
                notification("m1", issueId = "i1", teamId = "t1", type = DomainContract.notificationTypeAgentMessage),
                notification("c1", issueId = "i1", type = "issue_comment"),
                notification("c3", issueId = "i2", type = "issue_comment"),
                notification("c2", issueId = "i2", type = DomainContract.notificationTypeAgentMessage, readAt = ts),
            ),
            issues = listOf(issue("i1"), issue("i2")),
            teams = listOf(team("t1", "Acme")),
        )
        assertEquals(emptyList<InboxEntry.Message>(), state.entries.filterIsInstance<InboxEntry.Message>())
        val groups = state.issueGroups
        assertEquals(listOf("i1", "i2"), groups.map { it.issue.id })
        assertEquals(true, groups[0].opensResults)
        assertEquals(false, groups[1].opensResults)
    }

    /**
     * EXP-980: a blocked coding run is its own issue-less entry, carrying the
     * run its tap opens; a row whose run was pruned still renders and only
     * marks read.
     */
    @Test
    fun blockedRunsAreOneEntryEachAndCarryTheirRun() {
        val state = buildInboxState(
            notifications = listOf(
                notification(
                    "s1", teamId = "t1", sessionId = "run-1",
                    type = DomainContract.notificationTypeSessionBlocked,
                    title = "EXP-12 hit a rate limit", body = "Rate limited · resets in 2h",
                ),
                notification(
                    "s2", teamId = "t1", sessionId = null,
                    type = DomainContract.notificationTypeSessionBlocked, readAt = ts,
                ),
            ),
            issues = emptyList(),
            teams = listOf(team("t1", "Acme")),
        )

        val sessions = state.entries.filterIsInstance<InboxEntry.Session>()
        assertEquals(2, sessions.size)
        assertEquals("session:s1", sessions[0].key)
        assertEquals("Acme", sessions[0].teamName)
        assertEquals("EXP-12 hit a rate limit", sessions[0].notification.title)
        assertEquals("run-1", sessions[0].sessionId)
        assertEquals(1, sessions[0].unread)
        // The pruned run's row stays, with nothing to open.
        assertNull(sessions[1].sessionId)
        assertEquals(0, sessions[1].unread)
        assertEquals(1, state.totalUnread)
    }

    /**
     * Issue-less rows of any other type are dropped — a comment without an
     * issue, a reporter reply whose issue has not synced, and an issue id
     * the local issues table does not know.
     */
    @Test
    fun issueLessRowsOfOtherTypesStayDropped() {
        val state = buildInboxState(
            notifications = listOf(
                notification("n1", type = DomainContract.notificationTypeIssueComment),
                notification("n2", issueId = "ghost", type = DomainContract.notificationTypeReporterReply),
                notification("n3", teamId = "t1", type = DomainContract.notificationTypeReporterReply),
            ),
            issues = emptyList(),
            teams = listOf(team("t1", "Acme")),
        )
        assertTrue(state.entries.isEmpty())
        assertEquals(0, state.totalUnread)
    }

    /**
     * Web/iOS/desktop parity: the issue-less entries are NOT pinned above the
     * issue stream — the one feed order (newest-first) decides, so a stale
     * agent message sinks below fresher issue activity.
     */
    @Test
    fun entriesInterleaveByLatestActivity() {
        val state = buildInboxState(
            notifications = listOf(
                // Newest-first, like the DAO delivers.
                notification("n1", issueId = "i1"),
                notification("n2", teamId = "t1", type = DomainContract.notificationTypeAgentMessage),
                notification("n3", issueId = "i2"),
                // Older row of an already-seen group: must not re-order it.
                notification("n4", issueId = "i1", readAt = ts),
            ),
            issues = listOf(issue("i1"), issue("i2")),
            teams = listOf(team("t1", "Acme")),
        )

        assertEquals(
            listOf("issue:i1", "message:n2", "issue:i2"),
            state.entries.map { it.key },
        )
    }

    // Wire contract: notifications sync a nullable team_id (set on issue-less
    // agent_message / session_blocked rows, NULL on issue-anchored rows).
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    // SLOP-4: a reporter's reply is issue-anchored like a comment.
    @Test
    fun decodesReporterReplyRowWithIssueId() {
        val row = json.decodeFromString(
            NotificationEntity.serializer(),
            """
            {"id":"n1","user_id":"u1","issue_id":"i1","team_id":null,
             "type":"reporter_reply","title":"Emma Fischer replied on EXP-1",
             "body":"Still broken","read_at":null,
             "created_at":"$ts","updated_at":"$ts"}
            """.trimIndent(),
        )
        assertEquals("i1", row.issueId)
        assertNull(row.teamId)
        assertEquals(DomainContract.notificationTypeReporterReply, row.type)
    }

    // EXP-980: the shape gained a nullable session_id, set on session_blocked.
    @Test
    fun decodesSessionBlockedRowWithSessionId() {
        val row = json.decodeFromString(
            NotificationEntity.serializer(),
            """
            {"id":"s1","user_id":"u1","issue_id":null,"team_id":"t1","session_id":"run-1",
             "type":"session_blocked","title":"EXP-12 hit a rate limit",
             "body":"Rate limited · resets in 2h","read_at":null,
             "created_at":"$ts","updated_at":"$ts"}
            """.trimIndent(),
        )
        assertNull(row.issueId)
        assertEquals("t1", row.teamId)
        assertEquals("run-1", row.sessionId)
        assertEquals(DomainContract.notificationTypeSessionBlocked, row.type)
    }

    @Test
    fun decodesIssueAnchoredRowWithoutTeamId() {
        val row = json.decodeFromString(
            NotificationEntity.serializer(),
            """
            {"id":"n2","user_id":"u1","issue_id":"i1",
             "type":"issue_comment","title":"Ann commented",
             "created_at":"$ts","updated_at":"$ts"}
            """.trimIndent(),
        )
        assertEquals("i1", row.issueId)
        assertNull(row.teamId)
        assertNull(row.sessionId)
    }
}
