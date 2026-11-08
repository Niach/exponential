package com.exponential.app.ui.chat

import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTree.ChatTab
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-1343 / #EXP-1433: the phone pager's pages = Conversation · (the one
 *  issue | the item opened from the group) · Results (web `chat-pages.ts`). */
class ChatPagesTest {

    private val groupTabs = listOf(
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
        ChatTab.Results,
    )

    @Test
    fun theGroupIsNeverAPageUntilAnItemIsOpenedFromIt() {
        assertEquals(listOf("conversation", "results"), chatPages(groupTabs).map(::chatTabKey))
        assertEquals(
            listOf("conversation", "session:s2", "results"),
            chatPages(groupTabs, open = "session:s2").map(::chatTabKey),
        )
        assertEquals(
            listOf("conversation", "issue:i2", "results"),
            chatPages(groupTabs, open = "issue:i2").map(::chatTabKey),
        )
        // Not an item of the group: no page.
        assertEquals(listOf("conversation", "results"), chatPages(groupTabs, open = "issue:nope").map(::chatTabKey))
    }

    @Test
    fun theOpenItemLeavesThePagesOnceThePagerSettlesElsewhere() {
        // Web `chatPages`: the opened item is a page only while it is the
        // shown one; Android drops it when the pager rests on another page.
        assertEquals("session:s2", chatGroupOpenAfterSettle("session:s2", "session:s2"))
        assertNull(chatGroupOpenAfterSettle("session:s2", "conversation"))
        assertNull(chatGroupOpenAfterSettle("issue:i2", "results"))
        assertNull(chatGroupOpenAfterSettle(null, "conversation"))
        val open = chatGroupOpenAfterSettle("issue:i2", "results")
        assertEquals(listOf("conversation", "results"), chatPages(groupTabs, open = open).map(::chatTabKey))
    }

    @Test
    fun theGroupListsEveryIssueAndSessionNestedOnesIncluded() {
        val items = chatStripItems(groupTabs, null)
        assertEquals(listOf("i1", "i2", "i3"), items.issues.map { it.issueId })
        assertEquals(listOf("s1", "s2"), items.sessions.map { it.sessionId })
        assertEquals(SessionTree.StripMode.Group, items.mode.mode)
        assertEquals(5, items.mode.count)
        assertEquals("APP-2", items.label("issue:i2"))
        assertEquals("Measure", items.label("session:s2"))
        assertNull(items.label("conversation"))
    }

    @Test
    fun oneIssueAndNoSessionKeepsItsChipAndItsPage() {
        val tabs = listOf(ChatTab.Add, ChatTab.Conversation, ChatTab.IssueTab("i1", "APP-1"), ChatTab.Results)
        assertEquals(listOf("conversation", "issue:i1", "results"), chatPages(tabs).map(::chatTabKey))
        assertEquals(SessionTree.StripMode.Chips, chatStripItems(tabs, null).mode.mode)
    }

    @Test
    fun anAgentChannelOrNothingLoadedIsTheConversationAlone() {
        assertEquals(listOf<ChatTab>(ChatTab.Conversation), chatPages(null))
    }

    @Test
    fun aSingleSessionChatsConversationPageIsItsSessionPageInPlace() {
        val tabs = listOf(ChatTab.Add, ChatTab.Conversation, ChatTab.IssueTab("i1", "APP-1"))
        assertEquals(
            listOf("session:root", "issue:i1"),
            chatMainPages(chatPages(tabs, null, "root"), "root").map(::chatTabKey),
        )
    }

    @Test
    fun theMainSessionIsNeverAGroupRow() {
        val tabs = listOf(
            ChatTab.Conversation,
            ChatTab.IssueTab("i1", "APP-1"),
            ChatTab.SessionTab("root", "Root", SessionTree.SessionState.Working),
        )
        // The root excluded: one issue, no session = chips.
        assertEquals(
            listOf("session:root", "issue:i1"),
            chatMainPages(chatPages(tabs, null, "root"), "root").map(::chatTabKey),
        )
    }

    @Test
    fun noMainSessionKeepsThePagesAsTheyAre() {
        val tabs = listOf(ChatTab.Conversation, ChatTab.IssueTab("i1", "APP-1"))
        assertEquals(listOf("conversation", "issue:i1"), chatMainPages(chatPages(tabs), null).map(::chatTabKey))
    }

    @Test
    fun `the plan page follows Conversation`() {
        // #EXP-1662 (plan-page.json): web `chatPages`.
        val tabs = chatTabsWithPlan(listOf(ChatTab.Conversation, ChatTab.Add, ChatTab.IssueTab("i1", "EXP-1"), ChatTab.Results), hasPlan = true)
        assertEquals(listOf("conversation", "plan", "issue:i1", "results"), chatPages(tabs).map(::chatTabKey))
    }

    @Test
    fun `no plan no page`() {
        val tabs = chatTabsWithPlan(listOf(ChatTab.Conversation, ChatTab.Results), hasPlan = false)
        assertEquals(listOf("conversation", "results"), chatPages(tabs).map(::chatTabKey))
    }

    @Test
    fun `the shown plan is the opened one else the tab target`() {
        fun plan(id: String, decision: String?) = SessionTree.SessionChatRow.Plan(id = id, sessionId = "r", plan = "# $id", decision = decision)
        val rows = listOf(plan("a", "approved"), plan("b", null), plan("c", "rejected"))
        assertEquals("b", chatShownPlan(rows, null, emptyMap())?.id)
        assertEquals("a", chatShownPlan(rows, "a", emptyMap())?.id)
        assertEquals("b", chatShownPlan(rows, "gone", emptyMap())?.id)
        assertEquals(null, chatShownPlan(listOf(plan("s", "superseded")), null, emptyMap()))
        // #FEED-191: a decided plan leaves the strip unless this visit kept it.
        val decided = listOf(plan("a", "approved"), plan("c", "rejected"))
        assertEquals(null, chatShownPlan(decided, null, emptyMap()))
        assertEquals("a", chatShownPlan(decided, null, emptyMap(), kept = "a")?.id)
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
        assertEquals("child", chatShownPlan(rows, "child", mapOf("child" to child))?.id)
    }
}
