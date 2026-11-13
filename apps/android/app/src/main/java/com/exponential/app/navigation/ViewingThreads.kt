package com.exponential.app.navigation

import com.exponential.app.data.push.ViewingScreen
import com.exponential.app.domain.NotificationDelivery
import com.exponential.app.domain.SettingsNav

/**
 * #EXP-1815 (notification-delivery.json `viewing`): the thread of the screen
 * on top. An issue = `issue:<id>`, a chat = `session:<rootId>` (the chat route
 * IS a root), a session view = its root (the service resolves it), a device's
 * settings = `device:<id>`, the Inbox = `inbox`; anything else = none.
 */
object ViewingThreads {
    fun of(route: String?, arg: (String) -> String?): ViewingScreen? {
        fun id(name: String) = arg(name)?.takeIf { it.isNotEmpty() }
        return when (route) {
            ISSUE_ROUTE -> id("issueId")?.let { ViewingScreen.Thread("issue:$it") }
            Routes.CHAT_PATTERN -> id("sessionId")?.let { ViewingScreen.Thread("session:$it") }
            Routes.SESSION_PATTERN -> id("sessionId")?.let { ViewingScreen.Session(it) }
            Routes.TEAM_SETTINGS_PATTERN ->
                if (arg("section") == SettingsNav.DEVICES) id("device")?.let { ViewingScreen.Thread("device:$it") } else null
            MainTabs.INBOX -> ViewingScreen.Thread(NotificationDelivery.INBOX_THREAD)
            else -> null
        }
    }
}
