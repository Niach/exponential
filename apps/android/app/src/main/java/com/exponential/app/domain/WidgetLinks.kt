package com.exponential.app.domain

/**
 * #EXP-1830: the home widgets' `exponential://` links (iOS
 * `HomeWidgets.Links`, `LaunchTarget(url:)`). A widget launches MainActivity
 * with an EXPLICIT intent (`MainActivity.widgetIntent`) carrying one of these;
 * every target opens with THE back stack rule (app-shortcuts.json: the Agent
 * tab root under it). Android-free: [parse] takes the URL's parts.
 */
object WidgetLinks {
    const val SCHEME = "exponential"

    const val HOST_HOME = "home"
    const val HOST_MY_ISSUES = "my-issues"
    const val HOST_NEEDS_YOU = "needs-you"
    const val HOST_BOARD = "board"
    const val HOST_ISSUE = "issue"
    const val HOST_SESSION = "session"

    const val PARAM_OPENS = "opens"
    const val PARAM_MESSAGE = "message"

    sealed interface Target {
        /** THE start screen (a signed-out widget). */
        data object Home : Target
        /** The Board tab on its My issues chip. */
        data object MyIssues : Target
        /** THE Needs you page. */
        data object NeedsYou : Target
        data class Board(val id: String) : Target
        data class Issue(val id: String) : Target
        /** A needs-you row: its root opened by [opens] on [messageId]. */
        data class NeedsYouRow(
            val rootId: String,
            val messageId: String,
            val opens: com.exponential.app.domain.NeedsYou.Opens,
        ) : Target
    }

    fun board(id: String): String = "$SCHEME://$HOST_BOARD/$id"
    const val MY_ISSUES = "$SCHEME://$HOST_MY_ISSUES"
    const val NEEDS_YOU = "$SCHEME://$HOST_NEEDS_YOU"
    const val HOME = "$SCHEME://$HOST_HOME"
    fun issue(id: String): String = "$SCHEME://$HOST_ISSUE/$id"
    fun needsYouRow(rootId: String, messageId: String, opens: com.exponential.app.domain.NeedsYou.Opens): String =
        "$SCHEME://$HOST_SESSION/$rootId?$PARAM_OPENS=${opens.wire}&$PARAM_MESSAGE=${encodeRouteValue(messageId)}"

    /**
     * The widget target of `exponential://[host]/[pathSegments]?[query]`;
     * null = not a widget link (or one missing its id).
     */
    fun parse(host: String?, pathSegments: List<String>, query: (String) -> String?): Target? {
        val first = pathSegments.firstOrNull()?.takeIf { it.isNotEmpty() }
        return when (host) {
            HOST_HOME -> Target.Home
            HOST_MY_ISSUES -> Target.MyIssues
            HOST_NEEDS_YOU -> Target.NeedsYou
            HOST_BOARD -> first?.let { Target.Board(it) }
            HOST_ISSUE -> first?.let { Target.Issue(it) }
            HOST_SESSION -> {
                val opens = query(PARAM_OPENS)?.let { wire ->
                    com.exponential.app.domain.NeedsYou.Opens.entries.firstOrNull { it.wire == wire }
                }
                val message = query(PARAM_MESSAGE)?.takeIf { it.isNotEmpty() }
                if (first == null || opens == null || message == null) null else Target.NeedsYouRow(first, message, opens)
            }
            else -> null
        }
    }
}
