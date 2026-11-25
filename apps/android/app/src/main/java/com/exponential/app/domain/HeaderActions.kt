package com.exponential.app.domain

/**
 * #EXP-2109 (fixtures/header-actions.json, its `$comment` IS the spec; the pure
 * reference is `packages/domain-contract/src/header-actions.ts`): THE header
 * actions: one order, one round button, + never inside the `…`.
 * `HeaderActionsTest` replays it.
 */
object HeaderActions {
    /** Phones: the filled circle and its glyph (dp). */
    const val CIRCLE = 44
    const val GLYPH = 20
    const val GAP = 8

    val ORDER: List<String> = listOf("status", "primary", "call", "page", "search", "inbox", "add", "more")

    /** Concept names. */
    val ICONS: Map<String, String> = linkedMapOf(
        "call" to "voice-call",
        "search" to "nav-search",
        "inbox" to "nav-inbox",
        "add" to "ui-add",
        "more" to "ui-more",
    )

    val MENU_DROPS: List<String> =
        listOf("open", "open-new-tab", "move", "new", "new-issue", "new-conversation", "newConversation")

    const val SEPARATOR = "separator"

    fun order(actions: List<String>): List<String> = ORDER.filter { it in actions }

    /** A header's `…`: the page's menu without what the + does, separators
     *  tidied; empty = no button. */
    fun menu(entries: List<String>): List<String> {
        val kept = entries.filter { it !in MENU_DROPS }
        return kept.filterIndexed { index, entry ->
            entry != SEPARATOR || (index > 0 && index < kept.size - 1 && kept[index - 1] != SEPARATOR)
        }
    }
}
