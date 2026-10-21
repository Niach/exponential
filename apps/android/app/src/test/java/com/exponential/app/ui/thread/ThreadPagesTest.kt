package com.exponential.app.ui.thread

import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTree.ThreadTab
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-1343: the phone pager pages through EVERY strip tab, in strip order. */
class ThreadPagesTest {

    @Test
    fun everyStripTabIsAPageInStripOrderWithFoldedRowsAfterTheirChip() {
        val tabs = listOf(
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
            ThreadTab.SessionTab("s3", "Cache", SessionTree.SessionState.Ended),
            ThreadTab.Guide(dot = false),
        )
        assertEquals(
            listOf(
                "conversation", "issue:i1", "issue:i2", "issue:i3",
                "session:s1", "session:s2", "session:s3", "guide",
            ),
            threadPages(tabs).map(::threadTabKey),
        )
    }

    @Test
    fun aRepeatedTabIsOnePageNeverADuplicatePagerKey() {
        val tabs = listOf(
            ThreadTab.Conversation,
            ThreadTab.IssueTab("i1", "APP-1", nested = listOf(SessionTree.NestedIssue("i2", "APP-2", 1))),
            ThreadTab.IssueTab("i2", "APP-2"),
        )
        assertEquals(listOf("conversation", "issue:i1", "issue:i2"), threadPages(tabs).map(::threadTabKey))
    }

    @Test
    fun anAgentChannelOrNothingLoadedIsTheConversationAlone() {
        assertEquals(listOf<ThreadTab>(ThreadTab.Conversation), threadPages(null))
    }

    @Test
    fun aSingleSessionThreadLeadsWithItsSessionPageAndHasNoConversation() {
        val tabs = listOf(ThreadTab.Add, ThreadTab.IssueTab("i1", "APP-1"), ThreadTab.Guide(dot = false))
        assertEquals(
            listOf("session:root", "issue:i1", "guide"),
            threadMainPages(threadPages(tabs), "root").map(::threadTabKey),
        )
    }

    @Test
    fun noMainSessionKeepsThePagesAsTheyAre() {
        val tabs = listOf(ThreadTab.Conversation, ThreadTab.Guide(dot = false))
        assertEquals(listOf("conversation", "guide"), threadMainPages(threadPages(tabs), null).map(::threadTabKey))
    }
}
