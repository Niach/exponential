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
 * Inbox grouping (EXP-180 helpdesk parity): issue-less `support_reply`
 * notifications must form synthetic per-team Support groups instead of being
 * dropped, and totalUnread must include them. NULL/unknown team ids collapse
 * into one generic bucket.
 */
class InboxGroupingTest {

    private val ts = "2026-07-19 00:00:00+00"

    private fun notification(
        id: String,
        issueId: String? = null,
        teamId: String? = null,
        type: String = DomainContract.notificationTypeSupportReply,
        title: String = "title-$id",
        body: String? = null,
        readAt: String? = null,
    ) = NotificationEntity(
        id = id, userId = "u1", issueId = issueId, teamId = teamId, type = type,
        title = title, body = body, readAt = readAt, createdAt = ts, updatedAt = ts,
    )

    private fun issue(id: String) = IssueEntity(
        id = id, boardId = "b1", number = 1, identifier = "EXP-1", title = "Issue $id",
        status = "todo", priority = "none", creatorId = "u1", sortOrder = 1.0,
        createdAt = ts, updatedAt = ts,
    )

    private fun team(id: String, name: String) = TeamEntity(
        id = id, name = name, slug = id, createdAt = ts, updatedAt = ts,
    )

    @Test
    fun supportRepliesGroupPerTeamAndCountTowardTotalUnread() {
        val state = buildInboxState(
            notifications = listOf(
                // Newest-first, like the DAO delivers.
                notification("n1", teamId = "t1", title = "Ann replied on a support ticket"),
                notification("n2", issueId = "i1", type = DomainContract.notificationTypeIssueComment),
                notification("n3", teamId = "t1", readAt = ts),
                notification("n4", teamId = "t2", readAt = ts),
            ),
            issues = listOf(issue("i1")),
            teams = listOf(team("t1", "Acme"), team("t2", "Globex")),
        )

        assertEquals(1, state.groups.size)
        assertEquals(2, state.supportGroups.size)

        val acme = state.supportGroups.first { it.teamId == "t1" }
        assertEquals("Acme", acme.teamName)
        assertEquals(2, acme.notifications.size)
        assertEquals(1, acme.unread)
        // Newest-first preserved: the group's latest drives the row preview.
        assertEquals("Ann replied on a support ticket", acme.latest.title)

        val globex = state.supportGroups.first { it.teamId == "t2" }
        assertEquals("Globex", globex.teamName)
        assertEquals(0, globex.unread)

        // 1 unread issue notification + 1 unread support notification.
        assertEquals(2, state.totalUnread)
    }

    @Test
    fun nullAndUnknownTeamRowsCollapseIntoOneGenericGroup() {
        val state = buildInboxState(
            notifications = listOf(
                notification("n1", teamId = null),
                notification("n2", teamId = "ghost"), // not in the local teams table
            ),
            issues = emptyList(),
            teams = listOf(team("t1", "Acme")),
        )

        assertEquals(1, state.supportGroups.size)
        val generic = state.supportGroups.single()
        assertNull(generic.teamId)
        assertNull(generic.teamName)
        assertEquals(2, generic.notifications.size)
        assertEquals(2, generic.unread)
        assertEquals(2, state.totalUnread)
    }

    @Test
    fun issueLessNonSupportRowsStayDropped() {
        val state = buildInboxState(
            notifications = listOf(
                notification("n1", type = DomainContract.notificationTypeIssueComment),
            ),
            issues = emptyList(),
            teams = emptyList(),
        )
        assertTrue(state.groups.isEmpty())
        assertTrue(state.supportGroups.isEmpty())
        assertEquals(0, state.totalUnread)
    }

    // Wire contract: notifications now sync a nullable team_id (set on
    // issue-less support_reply rows, NULL on issue-anchored rows).
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    @Test
    fun decodesSupportReplyRowWithTeamId() {
        val row = json.decodeFromString(
            NotificationEntity.serializer(),
            """
            {"id":"n1","user_id":"u1","issue_id":null,"team_id":"t1",
             "type":"support_reply","title":"New support ticket from Ann",
             "body":"It broke","read_at":null,
             "created_at":"$ts","updated_at":"$ts"}
            """.trimIndent(),
        )
        assertNull(row.issueId)
        assertEquals("t1", row.teamId)
        assertEquals(DomainContract.notificationTypeSupportReply, row.type)
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
    }
}
