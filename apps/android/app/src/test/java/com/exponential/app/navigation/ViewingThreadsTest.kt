package com.exponential.app.navigation

import com.exponential.app.data.push.ViewingScreen
import com.exponential.app.domain.SettingsNav
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** #EXP-1815 (notification-delivery.json `viewing`): the screen on top's thread. */
class ViewingThreadsTest {
    private fun of(route: String?, vararg args: Pair<String, String>) = ViewingThreads.of(route) { mapOf(*args)[it] }

    @Test
    fun `each screen's thread`() {
        assertEquals(ViewingScreen.Thread("issue:i1"), of(ISSUE_ROUTE, "issueId" to "i1"))
        assertEquals(ViewingScreen.Thread("session:r1"), of(Routes.CHAT_PATTERN, "sessionId" to "r1"))
        assertEquals(ViewingScreen.Session("c1"), of(Routes.SESSION_PATTERN, "sessionId" to "c1"))
        assertEquals(
            ViewingScreen.Thread("device:d1"),
            of(Routes.TEAM_SETTINGS_PATTERN, "section" to SettingsNav.DEVICES, "device" to "d1"),
        )
        assertEquals(ViewingScreen.Thread("inbox"), of(MainTabs.INBOX))
    }

    @Test
    fun `any other screen has none`() {
        assertNull(of(Routes.TEAM_SETTINGS_PATTERN, "section" to SettingsNav.DEVICES))
        assertNull(of(Routes.TEAM_SETTINGS_PATTERN, "section" to SettingsNav.GENERAL))
        assertNull(of(MainTabs.CHATS))
        assertNull(of(null))
    }
}
