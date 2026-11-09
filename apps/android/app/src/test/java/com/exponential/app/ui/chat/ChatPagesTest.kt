package com.exponential.app.ui.chat

import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTree.ChatTab
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** #EXP-1732 (session-screen.json): THE session screen's Issues / Sessions
 *  rows out of the chat's tree, and the opened plan item. */
class ChatPagesTest {

    private val tabs = listOf(
        ChatTab.Add,
        ChatTab.Conversation,
        ChatTab.IssueTab("i1", "APP-1", nested = listOf(SessionTree.NestedIssue("i2", "APP-2", 1))),
        ChatTab.IssueTab("i3", "APP-3"),
        ChatTab.SessionTab(
            "s1",
            "Profile",
            SessionTree.SessionState.Working,
            nested = listOf(SessionTree.NestedSession("s2", "Measure", SessionTree.SessionState.NeedsYou, 1)),
        ),
    )

    @Test
    fun theTabsListEveryIssueAndSessionNestedOnesIncluded() {
        val items = sessionScreenItems(tabs, null)
        assertEquals(listOf("i1", "i2", "i3"), items.issues.map { it.issueId })
        assertEquals(listOf(0, 1, 0), items.issues.map { it.depth })
        assertEquals(listOf("s1", "s2"), items.sessions.map { it.sessionId })
        assertEquals(listOf(0, 1), items.sessions.map { it.depth })
        assertEquals("APP-2", items.issue("i2")?.identifier)
        assertEquals("Measure", items.session("s2")?.name)
        assertNull(items.session("nope"))
    }

    @Test
    fun theMainSessionIsNeverASessionsRow() {
        val single = listOf(
            ChatTab.Conversation,
            ChatTab.IssueTab("i1", "APP-1"),
            ChatTab.SessionTab("root", "Root", SessionTree.SessionState.Working),
        )
        assertEquals(emptyList<String>(), sessionScreenItems(single, "root").sessions.map { it.sessionId })
        assertEquals(listOf("i1"), sessionScreenItems(single, "root").issues.map { it.issueId })
    }

    @Test
    fun `the opened plan is the root's row else any plan of the tree`() {
        fun plan(id: String, decision: String?) = SessionTree.SessionChatRow.Plan(id = id, sessionId = "r", plan = "# $id", decision = decision)
        val rows = listOf(plan("a", "approved"), plan("b", null))
        assertEquals("a", chatOpenedPlan(rows, "a", emptyMap())?.id)
        assertNull(chatOpenedPlan(rows, "gone", emptyMap()))
        assertNull(chatOpenedPlan(rows, null, emptyMap()))
        // A child session's plan is no row of the root's: found in the tree's messages.
        val child = SessionTree.Message(
            id = "child",
            sessionId = "c1",
            authorKind = "agent",
            authorId = null,
            body = "",
            card = buildJsonObject {
                put("kind", "plan")
                put("plan", "# Child")
            },
            answeredAt = null,
            consumedAt = null,
            createdAt = "2027-01-15T08:00:00.000Z",
            authorSessionId = "c1",
        )
        assertEquals("child", chatOpenedPlan(rows, "child", mapOf("child" to child))?.id)
    }
}
