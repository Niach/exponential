package com.exponential.app.navigation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1210: the bottom bar's destinations are sibling ROOTS. Which tab is up
 * is read off the back stack (the topmost tab root, so a pushed detail keeps
 * its tab lit), and only a move between two tab roots is drawn as a swap.
 */
class TabNavigationTest {

    @Test
    fun `the Agent root alone is the Agent tab`() {
        assertEquals(MainTabs.AGENT, MainTabs.current(listOf(null, "agent-tab")))
    }

    @Test
    fun `a tab root on the Agent root is that tab`() {
        assertEquals(MainTabs.TEAM, MainTabs.current(listOf(null, "agent-tab", "team")))
        // Thread 58cb32df: the inbox is a page pushed inside a tab.
        assertEquals(MainTabs.THREADS, MainTabs.current(listOf(null, "agent-tab", "threads", "personal")))
    }

    @Test
    fun `a detail pushed inside a tab keeps that tab`() {
        assertEquals(
            MainTabs.ISSUES,
            MainTabs.current(listOf(null, "agent-tab", "home", "board/{boardId}", "issue/{issueId}?comment={comment}")),
        )
        assertEquals(
            MainTabs.AGENT,
            MainTabs.current(listOf(null, "agent-tab", "session/{sessionId}")),
        )
    }

    @Test
    fun `the tab on the Agent root wins over the Agent tab`() {
        val present = setOf("agent-tab", "threads")
        assertEquals(MainTabs.THREADS, MainTabs.current { it in present })
        assertEquals(MainTabs.AGENT, MainTabs.current { it == "agent-tab" })
        assertNull(MainTabs.current { false })
    }

    @Test
    fun `no tab before the graph has one`() {
        assertNull(MainTabs.current(listOf(null, "onboarding")))
        assertNull(MainTabs.current(emptyList()))
    }

    @Test
    fun `the tab routes are the bar's five destinations`() {
        // NEW-7: the Actions tab is gone (prompts replaced actions); thread
        // 58cb32df: Board · Devices · Chats · Team + the Agent root.
        assertEquals(
            setOf("agent-tab", "home", "agents", "threads", "team"),
            MainTabs.routes,
        )
    }

    @Test
    fun `only tab root to tab root is a swap`() {
        assertTrue(MainTabs.isRootSwap("agent-tab", "home"))
        assertTrue(MainTabs.isRootSwap("threads", "agent-tab"))
        // A push from a tab root and Back to it keep the slide.
        assertFalse(MainTabs.isRootSwap("home", "issue/{issueId}?comment={comment}"))
        assertFalse(MainTabs.isRootSwap("board/{boardId}", "home"))
        assertFalse(MainTabs.isRootSwap(null, "home"))
    }
}
