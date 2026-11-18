package com.exponential.app.ui.chat

import com.exponential.app.domain.FindInChat
import com.exponential.app.domain.SessionScreen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EDE-130: the bar's state and the `…` entry (find-in-chat.json `menuCases`). */
class FindInChatBarTest {

    @Test
    fun `search is the first entry in its own group for a chat`() {
        var found = 0
        val groups = sessionScreenMenuGroups(
            SessionScreen.Kind.Session,
            listOf(SessionScreen.MenuEntry.LinkIssue, SessionScreen.MenuEntry.Separator, SessionScreen.MenuEntry.Delete),
            onFind = { found += 1 },
        ) {}
        assertEquals(listOf(FindInChat.Copy.MENU), groups[0].map { it.label })
        assertEquals("${SessionScreen.TestIds.MENU}-search", groups[0][0].testTag)
        assertEquals(3, groups.size)
        groups[0][0].onClick()
        assertEquals(1, found)
    }

    @Test
    fun `search alone when the viewer has no other entry`() {
        val groups = sessionScreenMenuGroups(SessionScreen.Kind.Assistant, emptyList(), onFind = {}) {}
        assertEquals(listOf(listOf(FindInChat.Copy.MENU)), groups.map { g -> g.map { it.label } })
    }

    @Test
    fun `an app has no search and no handler means no entry`() {
        val app = sessionScreenMenuGroups(SessionScreen.Kind.App, listOf(SessionScreen.MenuEntry.Delete), onFind = {}) {}
        assertTrue(app.flatten().none { it.label == FindInChat.Copy.MENU })
        val none = sessionScreenMenuGroups(SessionScreen.Kind.Session, listOf(SessionScreen.MenuEntry.Delete)) {}
        assertTrue(none.flatten().none { it.label == FindInChat.Copy.MENU })
    }

    @Test
    fun `closing clears the marks and reopening shows the last query`() {
        val state = FindInChatState("chat-a")
        state.openBar()
        assertTrue(state.open)
        assertEquals("", state.raw)
        state.updateRaw("cache")
        state.query = "cache"
        state.report("2/5", "Searching older messages…")
        state.go(FindInChat.Step.Previous)
        state.go(FindInChat.Step.Next)
        assertEquals("next", state.step?.step?.wire)
        assertEquals(2, state.step?.n)
        state.close()
        assertFalse(state.open)
        assertEquals("", state.activeQuery)
        assertEquals("", state.counter)
        assertNull(state.status)
        assertNull(state.step)
        val reopened = FindInChatState("chat-a")
        reopened.openBar()
        assertEquals("cache", reopened.raw)
        assertEquals("", FindInChatState("chat-b").also { it.openBar() }.raw)
    }
}
