package com.exponential.app.navigation

import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavHostController

/**
 * EXP-1210, #EXP-2109 (chats.json `shell.tabs`): the docked bar's destinations
 * are ROOT siblings on the phone. CHATS is the stack's bottom (where the app
 * lands); every other tab root sits directly on it, never on another tab, and
 * switching between them is the standard bottom-nav switch: pop to the Chats
 * root SAVING the tab being left, then the target with its saved stack
 * RESTORED. No tab root carries a back button; system Back on another tab
 * returns to Chats, then leaves the app. (New is a popup, never a route.)
 */
object MainTabs {
    const val CHATS = "chats"
    const val TEAMS = "teams"
    const val ASSISTANT = "assistant"
    const val SETTINGS = "settings"
    /** Session 58cb32df: the inbox is a PAGE (the page header's inbox circle
     *  pushes it), not a tab root. */
    const val INBOX = "personal"

    val routes: Set<String> = setOf(CHATS, TEAMS, ASSISTANT, SETTINGS)

    /**
     * The tab whose stack is up, given which tab roots are [onStack]: the
     * other tab sitting on the Chats root if there is one (a switch always
     * pops to the Chats root first, so at most one does), else Chats. A
     * detail pushed inside a tab still reads as that tab. Null before the
     * graph has a tab (onboarding).
     *
     * Android-free on purpose: this is the unit-tested half.
     */
    fun current(onStack: (String) -> Boolean): String? =
        routes.firstOrNull { it != CHATS && onStack(it) } ?: CHATS.takeIf(onStack)

    /** [current] over a back stack's route patterns (bottom → top). */
    fun current(routes: List<String?>): String? = current { it in routes }

    /** The `shell.pages` page a route pattern shows (null = a detail: no bar). */
    fun page(route: String?): String? = when (route) {
        CHATS -> "chats"
        TEAMS -> "teams"
        ASSISTANT -> "assistant"
        SETTINGS -> "settings"
        Routes.TEAM_PATTERN -> "team"
        Routes.FOLDER_PATTERN -> "folder"
        Routes.BOARD_PATTERN -> "board"
        Routes.TEAM_SETTINGS_ROOT_PATTERN, Routes.TEAM_SETTINGS_PATTERN -> "team-settings"
        Routes.DEVICES_PATTERN -> "devices"
        Routes.ASSISTANT_SETTINGS, Routes.NOTIFICATION_SETTINGS, Routes.SERVER_PATTERN, Routes.SERVER_VOICE_PATTERN,
        Routes.ABOUT, Routes.SECURITY, Routes.SYNC_DIAGNOSTICS, Routes.LICENSES -> "settings-page"
        else -> null
    }

    /** A move between two tab roots — drawn with no transition at all. */
    fun isRootSwap(from: String?, to: String?): Boolean = from in routes && to in routes
}

/**
 * The one in-flight tab switch, so the NavHost can draw it with no
 * transition even when a side of it is not a tab root (a pushed board list
 * the bar rides on, or a restored detail on top of the target tab). Matched by
 * the entries' ids, so a later push or pop between the same screens animates
 * as usual.
 */
class TabSwitchMarker {
    private var fromId: String? = null
    private var toId: String? = null

    fun mark(from: NavBackStackEntry?, to: NavBackStackEntry?) {
        fromId = from?.id
        toId = to?.id
    }

    fun matches(initial: NavBackStackEntry, target: NavBackStackEntry): Boolean =
        initial.id == fromId && target.id == toId
}

/**
 * The tab whose stack is up ([MainTabs.current]), probed through the public
 * [NavHostController.getBackStackEntry] rather than the library-restricted
 * `currentBackStack`.
 */
fun NavHostController.currentTab(): String? = MainTabs.current { route ->
    runCatching { getBackStackEntry(route) }.isSuccess
}

/**
 * Wipe every tab's live AND saved stack, then land on a fresh Chats root —
 * a stack reset (another account signed in, an invite accepted) must not
 * let a later tab tap restore the previous account's or team's screens.
 */
fun NavHostController.resetToRoot() {
    MainTabs.routes.forEach { if (it != MainTabs.CHATS) clearBackStack(it) }
    navigate(MainTabs.CHATS) { popUpTo(MainTabs.CHATS) { inclusive = true } }
}

/**
 * Switch to the tab [route]. Re-selecting the tab you are on pops back to its
 * root; otherwise pop to the Chats root saving the current tab's stack, then
 * open [route] restoring its own ([restore] = false lands on the bare root,
 * for a link). Popped to the Chats ROUTE rather than
 * `graph.findStartDestination()`: the graph's start is `onboarding` for an
 * account that began in the wizard, which is never on the stack afterwards.
 */
fun NavHostController.selectTab(
    route: String,
    marker: TabSwitchMarker? = null,
    restore: Boolean = true,
) {
    val from = currentBackStackEntry
    if (currentTab() == route) {
        // A plain pop inside the tab (it animates like Back does).
        if (from?.destination?.route != route) popBackStack(route, inclusive = false)
        return
    }
    navigate(route) {
        popUpTo(MainTabs.CHATS) { saveState = true }
        launchSingleTop = true
        restoreState = restore
    }
    marker?.mark(from, currentBackStackEntry)
}
