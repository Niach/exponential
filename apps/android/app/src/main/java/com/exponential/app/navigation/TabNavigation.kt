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
 * leaves the app.
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
     * The tab a back stack (bottom → top route patterns) is on: the TOPMOST
     * tab root in it, so a detail pushed inside a tab still reads as that
     * tab. Null before the graph has a tab (onboarding).
     *
     * Android-free on purpose: this is the unit-tested half.
     */
    fun current(routes: List<String?>): String? = routes.lastOrNull { it in this.routes }

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
    val stack = currentBackStack.value
    val from = currentBackStackEntry
    if (MainTabs.current(stack.map { it.destination.route }) == route) {
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
