package com.exponential.app.domain

/**
 * #EXP-1638: the app icon's long-press menu, the Android mirror of
 * `packages/domain-contract/src/app-shortcuts.ts`, locked by
 * `fixtures/app-shortcuts.json` (`AppShortcutsTest`). The statics live in
 * `res/xml/shortcuts.xml`; the recent chats are dynamic shortcuts
 * (`DirectShareShortcuts`); MainActivity routes a tapped one by [parseAppShortcut].
 */
object AppShortcuts {
    const val NEW_ISSUE = "new-issue"
    const val NEW_CHAT = "new-chat"
    const val SEARCH = "search"

    const val NEW_ISSUE_LABEL = "New issue"
    const val NEW_CHAT_LABEL = "New chat"
    const val SEARCH_LABEL = "Search"

    const val NEW_ISSUE_ICON = "nav-create-issue"
    const val NEW_CHAT_ICON = "action-chat"
    const val SEARCH_ICON = "nav-search"
    const val CHAT_ICON = "action-chat"

    const val RECENT_CHAT_PREFIX = "recent-chat:"
    const val MAX_ENTRIES = 4

    val STATICS: List<String> = listOf(NEW_ISSUE, NEW_CHAT, SEARCH)

    data class Shortcut(val id: String, val title: String, val icon: String)

    private val STATIC = mapOf(
        NEW_ISSUE to Shortcut(NEW_ISSUE, NEW_ISSUE_LABEL, NEW_ISSUE_ICON),
        NEW_CHAT to Shortcut(NEW_CHAT, NEW_CHAT_LABEL, NEW_CHAT_ICON),
        SEARCH to Shortcut(SEARCH, SEARCH_LABEL, SEARCH_ICON),
    )

    sealed interface Target {
        data object NewIssue : Target
        data object NewChat : Target
        data object Search : Target
        data class Chat(val id: String) : Target
    }

    /** The statics, then the most recent plain chats, capped at [MAX_ENTRIES]. */
    fun appShortcuts(chats: List<ShareTargets.Chat>): List<Shortcut> {
        val statics = STATICS.map { STATIC.getValue(it) }
        val recent = chats
            .filter { it.agentId.isNullOrEmpty() && !it.sealed }
            .sortedByDescending { it.updatedAt }
            .map { Shortcut("$RECENT_CHAT_PREFIX${it.id}", it.title, CHAT_ICON) }
        return (statics + recent).take(MAX_ENTRIES)
    }

    /** The destination a shortcut id names, null = the normal start screen. */
    fun parseAppShortcut(id: String): Target? = when (id) {
        NEW_ISSUE -> Target.NewIssue
        NEW_CHAT -> Target.NewChat
        SEARCH -> Target.Search
        else -> id.takeIf { it.startsWith(RECENT_CHAT_PREFIX) }
            ?.removePrefix(RECENT_CHAT_PREFIX)
            ?.takeIf { it.isNotEmpty() }
            ?.let { Target.Chat(it) }
    }
}
