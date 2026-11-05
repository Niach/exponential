package com.exponential.app.ui.thread

import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTree.ThreadTab
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-1343 / #EXP-1433: the phone pager's pages = Conversation · (the one
 *  issue | the item opened from the group) · Results (web `thread-pages.ts`). */
class ThreadPagesTest {

    private val groupTabs = listOf(
        ThreadTab.Add,
        ThreadTab.Conversation,
        ThreadTab.IssueTab("i1", "APP-1", nested = listOf(SessionTree.NestedIssue("i2", "APP-2", 1))),
        ThreadTab.IssueTab("i3", "APP-3"),
        ThreadTab.SessionTab(
            "s1",
            "Profile",
            SessionTree.SessionState.Working,
            nested = listOf(SessionTree.NestedSession("s2", "Measure", SessionTree.SessionState.NeedsYou, 1)),
        ),
        ThreadTab.Results,
    )

    @Test
    fun theGroupIsNeverAPageUntilAnItemIsOpenedFromIt() {
        assertEquals(listOf("conversation", "results"), threadPages(groupTabs).map(::threadTabKey))
        assertEquals(
            listOf("conversation", "session:s2", "results"),
            threadPages(groupTabs, open = "session:s2").map(::threadTabKey),
        )
        assertEquals(
            listOf("conversation", "issue:i2", "results"),
            threadPages(groupTabs, open = "issue:i2").map(::threadTabKey),
        )
        // Not an item of the group: no page.
        assertEquals(listOf("conversation", "results"), threadPages(groupTabs, open = "issue:nope").map(::threadTabKey))
    }

    @Test
    fun theOpenItemLeavesThePagesOnceThePagerSettlesElsewhere() {
        // Web `threadPages`: the opened item is a page only while it is the
        // shown one; Android drops it when the pager rests on another page.
        assertEquals("session:s2", threadGroupOpenAfterSettle("session:s2", "session:s2"))
        assertNull(threadGroupOpenAfterSettle("session:s2", "conversation"))
        assertNull(threadGroupOpenAfterSettle("issue:i2", "results"))
        assertNull(threadGroupOpenAfterSettle(null, "conversation"))
        val open = threadGroupOpenAfterSettle("issue:i2", "results")
        assertEquals(listOf("conversation", "results"), threadPages(groupTabs, open = open).map(::threadTabKey))
    }

    @Test
    fun theGroupListsEveryIssueAndSessionNestedOnesIncluded() {
        val items = threadStripItems(groupTabs, null)
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
        val tabs = listOf(ThreadTab.Add, ThreadTab.Conversation, ThreadTab.IssueTab("i1", "APP-1"), ThreadTab.Results)
        assertEquals(listOf("conversation", "issue:i1", "results"), threadPages(tabs).map(::threadTabKey))
        assertEquals(SessionTree.StripMode.Chips, threadStripItems(tabs, null).mode.mode)
    }

    @Test
    fun anAgentChannelOrNothingLoadedIsTheConversationAlone() {
        assertEquals(listOf<ThreadTab>(ThreadTab.Conversation), threadPages(null))
    }

    @Test
    fun aSingleSessionThreadsConversationPageIsItsSessionPageInPlace() {
        val tabs = listOf(ThreadTab.Add, ThreadTab.Conversation, ThreadTab.IssueTab("i1", "APP-1"))
        assertEquals(
            listOf("session:root", "issue:i1"),
            threadMainPages(threadPages(tabs, null, "root"), "root").map(::threadTabKey),
        )
    }

    @Test
    fun theMainSessionIsNeverAGroupRow() {
        val tabs = listOf(
            ThreadTab.Conversation,
            ThreadTab.IssueTab("i1", "APP-1"),
            ThreadTab.SessionTab("root", "Root", SessionTree.SessionState.Working),
        )
        // The root excluded: one issue, no session = chips.
        assertEquals(
            listOf("session:root", "issue:i1"),
            threadMainPages(threadPages(tabs, null, "root"), "root").map(::threadTabKey),
        )
    }

    @Test
    fun noMainSessionKeepsThePagesAsTheyAre() {
        val tabs = listOf(ThreadTab.Conversation, ThreadTab.IssueTab("i1", "APP-1"))
        assertEquals(listOf("conversation", "issue:i1"), threadMainPages(threadPages(tabs), null).map(::threadTabKey))
    }

    @Test
    fun `the plan page follows Conversation`() {
        // #EXP-1662 (plan-page.json): web `threadPages`.
        val tabs = threadTabsWithPlan(listOf(ThreadTab.Conversation, ThreadTab.Add, ThreadTab.IssueTab("i1", "EXP-1"), ThreadTab.Results), hasPlan = true)
        assertEquals(listOf("conversation", "plan", "issue:i1", "results"), threadPages(tabs).map(::threadTabKey))
    }

    @Test
    fun `no plan no page`() {
        val tabs = threadTabsWithPlan(listOf(ThreadTab.Conversation, ThreadTab.Results), hasPlan = false)
        assertEquals(listOf("conversation", "results"), threadPages(tabs).map(::threadTabKey))
    }

    @Test
    fun `the shown plan is the opened one else the tab target`() {
        fun plan(id: String, decision: String?) = SessionTree.ThreadRow.Plan(id = id, sessionId = "r", plan = "# $id", decision = decision)
        val rows = listOf(plan("a", "approved"), plan("b", null), plan("c", "rejected"))
        assertEquals("b", threadShownPlan(rows, null, emptyMap())?.id)
        assertEquals("a", threadShownPlan(rows, "a", emptyMap())?.id)
        assertEquals("b", threadShownPlan(rows, "gone", emptyMap())?.id)
        assertEquals(null, threadShownPlan(listOf(plan("s", "superseded")), null, emptyMap()))
        // #FEED-191: a decided plan leaves the strip unless this visit kept it.
        val decided = listOf(plan("a", "approved"), plan("c", "rejected"))
        assertEquals(null, threadShownPlan(decided, null, emptyMap()))
        assertEquals("a", threadShownPlan(decided, null, emptyMap(), kept = "a")?.id)
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
        assertEquals("child", threadShownPlan(rows, "child", mapOf("child" to child))?.id)
    }
}
