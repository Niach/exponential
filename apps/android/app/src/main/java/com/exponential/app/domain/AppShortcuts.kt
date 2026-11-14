package com.exponential.app.domain

/**
 * #EXP-1638 / #EXP-1830: the app icon's long-press menu, the Android mirror of
 * `packages/domain-contract/src/app-shortcuts.ts`, locked by
 * `fixtures/app-shortcuts.json` (`AppShortcutsTest`). The statics live in
 * `res/xml/shortcuts.xml`; the chat entries (the Assistant, pins, recent
 * chats) are dynamic shortcuts (`DirectShareShortcuts`); MainActivity routes a
 * tapped one by [parseAppShortcut], a sharing shortcut's launcher intent by
 * [launcherShortcutId].
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
    /** THE brand mark, never a stored icon. */
    const val ASSISTANT_ICON = "assistant"
    /** An agent entry whose agent has no icon. */
    const val AGENT_ICON = "bot"

    const val RECENT_CHAT_PREFIX = "recent-chat:"
    const val AGENT_PREFIX = "agent:"
    const val MAX_ENTRIES = 4

    val STATICS: List<String> = listOf(NEW_ISSUE, NEW_CHAT, SEARCH)

    data class Shortcut(val id: String, val title: String, val icon: String, val color: String? = null)

    /** An agent the viewer sees (the agent entry of its conversation). */
    data class Agent(val id: String, val name: String, val icon: String?, val color: String?)

    /** The viewer's Assistant (agent-settings.json `assistant`). */
    data class Assistant(val id: String, val name: String)

    data class Input(
        val assistant: Assistant?,
        val agents: List<Agent>,
        /** The viewer's session pins, in sidebar order. */
        val pins: List<String>,
        val chats: List<ShareTargets.Chat>,
    )

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
        /** #EXP-1830: THE agent's conversation (`agent-dm/{agentId}`). */
        data class Agent(val id: String) : Target
    }

    /** #EXP-1830: the chat entries: the Assistant, pins, then recent chats, deduplicated. */
    fun chatEntries(input: Input): List<Shortcut> {
        val agents = input.agents.associateBy { it.id }
        fun entry(chat: ShareTargets.Chat): Shortcut? {
            val agentId = chat.agentId
            if (!agentId.isNullOrEmpty()) {
                val agent = agents[agentId] ?: return null
                if (agent.id == input.assistant?.id) return assistantEntry(input.assistant)
                return Shortcut("$AGENT_PREFIX${agent.id}", agent.name, agent.icon ?: AGENT_ICON, agent.color)
            }
            if (chat.sealed) return null
            return Shortcut("$RECENT_CHAT_PREFIX${chat.id}", chat.title, CHAT_ICON)
        }
        val byId = input.chats.associateBy { it.id }
        val pinned = input.pins.mapNotNull { byId[it] }
        val recent = input.chats.sortedByDescending { it.updatedAt }
        val out = ArrayList<Shortcut>()
        val seen = HashSet<String>()
        fun push(item: Shortcut?) {
            if (item == null || !seen.add(item.id)) return
            out += item
        }
        input.assistant?.let { push(assistantEntry(it)) }
        pinned.forEach { push(entry(it)) }
        recent.forEach { push(entry(it)) }
        return out
    }

    private fun assistantEntry(assistant: Assistant): Shortcut =
        Shortcut("$AGENT_PREFIX${assistant.id}", assistant.name, ASSISTANT_ICON)

    /** The statics, then the chat entries, capped at [MAX_ENTRIES]. */
    fun appShortcuts(input: Input): List<Shortcut> {
        val statics = STATICS.map { STATIC.getValue(it) }
        return (statics + chatEntries(input)).take(MAX_ENTRIES)
    }

    /** The destination a shortcut id names, null = the normal start screen. */
    fun parseAppShortcut(id: String): Target? = when {
        id == NEW_ISSUE -> Target.NewIssue
        id == NEW_CHAT -> Target.NewChat
        id == SEARCH -> Target.Search
        id.startsWith(AGENT_PREFIX) -> id.removePrefix(AGENT_PREFIX).takeIf { it.isNotEmpty() }?.let { Target.Agent(it) }
        id.startsWith(RECENT_CHAT_PREFIX) ->
            id.removePrefix(RECENT_CHAT_PREFIX).takeIf { it.isNotEmpty() }?.let { Target.Chat(it) }
        else -> null
    }

    /** #EXP-1830: the app shortcut id a shortcut's LAUNCHER intent carries
     *  (a sharing shortcut dragged onto the home screen opens its chat). */
    fun launcherShortcutId(id: String): String? = when {
        id.startsWith(ShareTargets.AGENT_PREFIX) ->
            id.removePrefix(ShareTargets.AGENT_PREFIX).takeIf { it.isNotEmpty() }?.let { "$AGENT_PREFIX$it" }
        id.startsWith(ShareTargets.CHAT_PREFIX) ->
            id.removePrefix(ShareTargets.CHAT_PREFIX).takeIf { it.isNotEmpty() }?.let { "$RECENT_CHAT_PREFIX$it" }
        else -> id.takeIf { parseAppShortcut(it) != null }
    }
}
