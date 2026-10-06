package com.exponential.app.navigation

import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavHostController

/**
 * EXP-1210: the bottom bar's destinations are ROOT siblings on the phone. The
 * Agent tab is the stack's bottom (where the app lands); every other tab root
 * sits directly on it, never on another tab, and switching between them is
 * the standard bottom-nav switch: pop to the Agent root SAVING the tab being
 * left, then the target with its saved stack RESTORED. No tab root carries a
 * back button; system Back on a non-Agent tab returns to the Agent root, then
 * leaves the app. A tab's pushes are SAVED across a switch and restored on
 * return, except the Agent tab's: it is the `popUpTo` target, so it always
 * lands on its root (iOS `MainTab.keepsPushes` mirrors this).
 */
object MainTabs {
    /** The Agent tab — the stack root and launch landing. */
    const val AGENT = "agent-tab"
    const val ISSUES = "home"
    const val INBOX = "personal"
    const val DEVICES = "agents"
    const val REVIEWS = "reviews"
    const val ACTIONS = "actions"

    val routes: Set<String> = setOf(AGENT, ISSUES, INBOX, DEVICES, REVIEWS, ACTIONS)

    /**
     * The tab whose stack is up, given which tab roots are [onStack]: the
     * other tab sitting on the Agent root if there is one (a switch always
     * pops to the Agent root first, so at most one does), else the Agent
     * tab. A detail pushed inside a tab still reads as that tab. Null before
     * the graph has a tab (onboarding).
     *
     * Android-free on purpose: this is the unit-tested half.
     */
    fun current(onStack: (String) -> Boolean): String? =
        routes.firstOrNull { it != AGENT && onStack(it) } ?: AGENT.takeIf(onStack)

    /** [current] over a back stack's route patterns (bottom → top). */
    fun current(routes: List<String?>): String? = current { it in routes }

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
 * Wipe every tab's live AND saved stack, then land on a fresh Agent root —
 * a stack reset (another account signed in, an invite accepted) must not
 * let a later tab tap restore the previous account's or team's screens.
 */
fun NavHostController.resetToAgentRoot() {
    MainTabs.routes.forEach { if (it != MainTabs.AGENT) clearBackStack(it) }
    navigate(MainTabs.AGENT) { popUpTo(MainTabs.AGENT) { inclusive = true } }
}

/**
 * Switch to the tab [route]. Re-selecting the tab you are on pops back to its
 * root; otherwise pop to the Agent root saving the current tab's stack, then
 * open [route] restoring its own ([restore] = false lands on the bare root,
 * for a link). Popped to the Agent ROUTE rather than
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
        popUpTo(MainTabs.AGENT) { saveState = true }
        launchSingleTop = true
        restoreState = restore
    }
    marker?.mark(from, currentBackStackEntry)
}
