package com.exponential.app.navigation

import androidx.navigation.NavHostController

/**
 * Route plumbing for the deep-link drain in [AppNavHost] (EXP-528).
 *
 * Push taps used to navigate with launchSingleTop, which does not push: it
 * REPLACES the top back-stack entry through NavBackStackEntry's copy
 * constructor, and that copy keeps the previous entry's id — hence its
 * ViewModelStore. Every detail ViewModel that snapshots its route argument
 * once at construction (IssueDetailViewModel.issueId and
 * SupportThreadViewModel.threadId, both read from SavedStateHandle) was
 * therefore handed straight back with the PREVIOUS argument, so a push tapped
 * while ANOTHER issue was open re-rendered that other issue. Reading the
 * argument reactively inside the ViewModel cannot fix it either: the stale
 * SavedStateHandle belongs to the reused ViewModel, not to the replacement
 * entry.
 *
 * So deep links push a real entry — new id, new ViewModelStore, fresh
 * ViewModel — which is what in-app issue-to-issue navigation and the iOS
 * navigator already do, and the one thing launchSingleTop was there for (a
 * re-tap of the notification for what is already on screen) moves here, as a
 * comparison of CONCRETE routes.
 */
object DeepLinkRoutes {

    /**
     * [pattern] with every {placeholder} replaced by [argument]'s value for
     * that name, i.e. the concrete route a navigate() call would produce.
     * Null when the pattern is null, malformed, or an argument is missing —
     * an incomplete fill must never compare equal to a real route, because
     * "unknown top of stack" has to mean "push".
     *
     * Android-free on purpose: this is the unit-tested half.
     */
    fun concreteRoute(pattern: String?, argument: (String) -> String?): String? {
        if (pattern == null) return null
        if (!pattern.contains('{')) return pattern
        val filled = StringBuilder()
        var cursor = 0
        while (cursor < pattern.length) {
            val open = pattern.indexOf('{', cursor)
            if (open < 0) {
                filled.append(pattern.substring(cursor))
                break
            }
            val close = pattern.indexOf('}', open)
            if (close < 0) return null
            filled.append(pattern.substring(cursor, open))
            filled.append(argument(pattern.substring(open + 1, close)) ?: return null)
            cursor = close + 1
        }
        return filled.toString()
    }

    /** Whether the top entry ([pattern] filled from [argument]) already IS [route]. */
    fun isOnTop(pattern: String?, argument: (String) -> String?, route: String): Boolean =
        concreteRoute(pattern, argument) == route
}

/**
 * Navigate to [route] for a deep-link tap, unless that exact concrete route is
 * already on top — a re-tap of the notification for what is on screen — in
 * which case do nothing (EXP-528). Deliberately NOT launchSingleTop: see
 * [DeepLinkRoutes]. A null top entry (no graph yet on a cold launch) pushes,
 * which is the safe default.
 *
 * The comparison is the raw target route against the top entry's pattern
 * refilled from its DECODED arguments. Every deep-link argument today is an
 * opaque URL-safe value (issue/thread uuid, invite token), so both forms are
 * byte-equal; an argument that could carry / or % would have to be encoded
 * here first.
 */
fun NavHostController.navigateDeepLink(route: String) {
    val top = currentBackStackEntry
    val alreadyOnTop = DeepLinkRoutes.isOnTop(
        pattern = top?.destination?.route,
        argument = { name -> top?.arguments?.getString(name) },
        route = route,
    )
    if (alreadyOnTop) return
    navigate(route)
}
