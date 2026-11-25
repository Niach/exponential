package com.exponential.app.navigation

import com.exponential.app.domain.Chats
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1210, #EXP-2109: the docked bar's destinations are sibling ROOTS. Which tab
 * is up is read off the back stack (the topmost tab root, so a pushed detail
 * keeps its tab), and only a move between two tab roots is drawn as a swap.
 */
class TabNavigationTest {

    @Test
    fun `the Chats root alone is the Chats tab`() {
        assertEquals(MainTabs.CHATS, MainTabs.current(listOf(null, "chats")))
    }

    @Test
    fun `a tab root on the Chats root is that tab`() {
        assertEquals(MainTabs.TEAMS, MainTabs.current(listOf(null, "chats", "teams")))
        assertEquals(MainTabs.SETTINGS, MainTabs.current(listOf(null, "chats", "settings", "personal")))
    }

    @Test
    fun `a detail pushed inside a tab keeps that tab`() {
        assertEquals(
            MainTabs.TEAMS,
            MainTabs.current(listOf(null, "chats", "teams", Routes.TEAM_PATTERN, Routes.BOARD_PATTERN, ISSUE_ROUTE)),
        )
        assertEquals(MainTabs.CHATS, MainTabs.current(listOf(null, "chats", "session/{sessionId}")))
    }

    @Test
    fun `no tab before the graph has one`() {
        assertNull(MainTabs.current(listOf(null, "onboarding")))
        assertNull(MainTabs.current(emptyList()))
    }

    @Test
    fun `the tab routes are the bar's, New is a popup`() {
        assertEquals(Chats.Shell.TABS.toSet() - "new", MainTabs.routes)
    }

    @Test
    fun `only tab root to tab root is a swap`() {
        assertTrue(MainTabs.isRootSwap("chats", "teams"))
        assertTrue(MainTabs.isRootSwap("assistant", "chats"))
        // A push from a tab root and Back to it keep the slide.
        assertFalse(MainTabs.isRootSwap("chats", "issue/{issueId}?comment={comment}"))
        assertFalse(MainTabs.isRootSwap("board/{boardId}", "teams"))
        assertFalse(MainTabs.isRootSwap(null, "chats"))
    }

    @Test
    fun `a page lights its tab, a detail none`() {
        fun lit(route: String?) = Chats.Shell.page(MainTabs.page(route))
        assertEquals(Chats.Shell.Page("chats", true), lit("chats"))
        assertEquals(Chats.Shell.Page("teams", true), lit(Routes.FOLDER_PATTERN))
        assertEquals(Chats.Shell.Page("teams", true), lit(Routes.BOARD_PATTERN))
        assertEquals(Chats.Shell.Page("teams", true), lit(Routes.TEAM_SETTINGS_PATTERN))
        assertEquals(Chats.Shell.Page("settings", true), lit(Routes.DEVICES_PATTERN))
        assertEquals(Chats.Shell.Page("settings", true), lit(Routes.ASSISTANT_SETTINGS))
        assertEquals(Chats.Shell.Page("assistant", true), lit("assistant"))
        assertEquals(Chats.Shell.Page(null, false), lit(ISSUE_ROUTE))
        assertEquals(Chats.Shell.Page(null, false), lit(Routes.CHAT_PATTERN))
        // Every page of the fixture has a route here.
        val routes = listOf(
            "chats", "teams", "assistant", "settings", Routes.TEAM_PATTERN, Routes.FOLDER_PATTERN, Routes.BOARD_PATTERN,
            Routes.TEAM_SETTINGS_ROOT_PATTERN, Routes.DEVICES_PATTERN, Routes.ABOUT,
        )
        assertEquals(Chats.Shell.PAGES.keys - "device", routes.mapNotNull { MainTabs.page(it) }.toSet())
    }

    @Test
    fun `the Assistant's chat and agent routes land on its tab`() {
        val ref = AssistantRef("a1", "s1")
        assertEquals(AssistantLanding(), AssistantTab.landingOf(Routes.chat("s1"), ref))
        assertEquals(AssistantLanding(card = "m1"), AssistantTab.landingOf(Routes.chatCard("s1", "m1"), ref))
        assertEquals(AssistantLanding(tab = "plan", plan = "m1"), AssistantTab.landingOf(Routes.chatPlan("s1", "m1"), ref))
        assertEquals(AssistantLanding(openSettings = true), AssistantTab.landingOf(Routes.agentDm("a1", "settings"), ref))
        assertNull(AssistantTab.landingOf(Routes.chat("s2"), ref))
        assertNull(AssistantTab.landingOf(Routes.agentDm("a2"), ref))
        assertNull(AssistantTab.landingOf(Routes.chat("s1"), null))
    }
}
