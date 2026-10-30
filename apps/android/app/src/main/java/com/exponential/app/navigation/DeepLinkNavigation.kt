package com.exponential.app.navigation

import androidx.navigation.NavHostController
import com.exponential.app.domain.InboxRoute

/**
 * Route plumbing for the deep-link drain in [AppNavHost] (EXP-528).
 *
 * Push taps used to navigate with launchSingleTop, which does not push: it
 * REPLACES the top back-stack entry through NavBackStackEntry's copy
 * constructor, and that copy keeps the previous entry's id — hence its
 * ViewModelStore. Every detail ViewModel that snapshots its route argument
 * once at construction (IssueDetailViewModel.issueId, read from
 * SavedStateHandle) was
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
 * opaque URL-safe value (issue/session uuid, invite token), so both forms are
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

/**
 * [navigateDeepLink] for the issue route: its optional `comment` query
 * argument is absent on a plain issue link, which [DeepLinkRoutes] reads as
 * an incomplete fill (always push). So the re-tap check compares the
 * ARGUMENTS instead: the same issue (and comment) is already on top.
 */
fun NavHostController.navigateIssueDeepLink(issueId: String, commentId: String? = null) {
    val top = currentBackStackEntry
    val alreadyOnTop = top?.destination?.route == ISSUE_ROUTE &&
        top.arguments?.getString("issueId") == issueId &&
        top.arguments?.getString("comment") == commentId
    if (alreadyOnTop) return
    navigate(issueRoute(issueId, commentId))
}

/** The issue screen's route pattern — (EXP-1241) `comment` is an OPTIONAL
 *  query arg. */
const val ISSUE_ROUTE = "issue/{issueId}?comment={comment}"

/** EXP-1241: the issue route scrolled to [commentId] (null = the plain route). */
fun issueRoute(issueId: String, commentId: String? = null): String =
    if (commentId == null) "issue/$issueId" else "issue/$issueId?comment=$commentId"

/**
 * #EXP-1456: the route a session opens on (fixtures/inbox-route.json
 * `sessionCases`, the S-3 rule) — an inbox row's and a push's session tap.
 * My own agent conversation root = the agent's channel (the thread screen IS
 * an agent root's channel, where the waiting question or plan docks); a
 * thread root = the thread; a thread's child and anything else = the session
 * view (Android has no in-thread child route, so a child opens on its own
 * screen). [target] null = not synced: the session route's not-found state.
 */
fun sessionTargetRoute(sessionId: String, target: InboxRoute.SessionTarget?): String =
    when (target) {
        // The conversation root IS the session (sessionTarget only says
        // `agent` for a root), and the thread screen is its channel.
        is InboxRoute.SessionTarget.Agent -> Routes.thread(sessionId)
        is InboxRoute.SessionTarget.Thread ->
            target.sessionId?.let(Routes::session) ?: Routes.thread(target.threadId)
        is InboxRoute.SessionTarget.Session -> Routes.session(target.sessionId)
        null -> Routes.session(sessionId)
    }
