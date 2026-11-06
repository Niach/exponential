package com.exponential.app.domain

/**
 * #EXP-1642: issues and chats in the phone's own search, the Android mirror
 * of `packages/domain-contract/src/system-search.ts`, locked by
 * `fixtures/system-search.json` (`SystemSearchTest`). The fixture's
 * `$comment` holds the rules; `data/search/SystemSearchIndexer` writes the
 * items into AppSearch, MainActivity opens a result by [parseSystemSearchId].
 */
object SystemSearch {
    const val ISSUE_SUBTITLE = "{identifier} · {board}"
    const val CHAT_SUBTITLE = "Chat"
    const val ISSUE_PREFIX = "issue:"
    const val CHAT_PREFIX = "chat:"
    const val DOMAIN_PREFIX = "account:"
    const val DEBOUNCE_MS = 2_000L

    data class Caps(val maxIssues: Int, val maxChats: Int)

    /** The fixture's tiny caps (the replay). */
    val FIXTURE_CAPS = Caps(maxIssues = 2, maxChats = 2)
    /** What the app indexes. */
    val PRODUCTION_CAPS = Caps(maxIssues = 1000, maxChats = 200)

    data class Board(val id: String, val name: String)

    data class Issue(
        val id: String,
        val identifier: String,
        val title: String,
        val boardId: String,
        val updatedAt: String,
        val trashed: Boolean = false,
        val archived: Boolean = false,
    )

    data class Item(val id: String, val title: String, val subtitle: String, val keywords: List<String>)

    enum class Kind { Issue, Chat }

    data class Target(val kind: Kind, val id: String)

    /** One account's domain (a sign-out deletes it and nothing else). */
    fun domain(accountId: String): String = "$DOMAIN_PREFIX$accountId"

    /** The account a [domain] names, null = not one of ours. */
    fun accountOf(domain: String): String? =
        domain.takeIf { it.startsWith(DOMAIN_PREFIX) }?.removePrefix(DOMAIN_PREFIX)?.takeIf { it.isNotEmpty() }

    /** The index of one account: live issues, then plain chats, each capped. */
    fun systemSearchItems(
        boards: List<Board>,
        issues: List<Issue>,
        chats: List<ShareTargets.Chat>,
        caps: Caps = FIXTURE_CAPS,
    ): List<Item> {
        val names = boards.associate { it.id to it.name }
        val issueItems = issues
            .filter { !it.trashed && !it.archived }
            .sortedByDescending { it.updatedAt }
            .take(caps.maxIssues)
            .map { issue ->
                val board = names[issue.boardId]
                Item(
                    id = "$ISSUE_PREFIX${issue.id}",
                    title = issue.title,
                    subtitle = board?.let {
                        ISSUE_SUBTITLE.replace("{identifier}", issue.identifier).replace("{board}", it)
                    } ?: issue.identifier,
                    keywords = if (board != null) listOf(issue.identifier, board) else listOf(issue.identifier),
                )
            }
        val chatItems = chats
            .filter { it.agentId.isNullOrEmpty() && !it.sealed }
            .sortedByDescending { it.updatedAt }
            .take(caps.maxChats)
            .map { Item("$CHAT_PREFIX${it.id}", it.title, CHAT_SUBTITLE, emptyList()) }
        return issueItems + chatItems
    }

    /** The destination a result id names, null = the normal start screen. */
    fun parseSystemSearchId(id: String): Target? {
        val (kind, prefix) = when {
            id.startsWith(ISSUE_PREFIX) -> Kind.Issue to ISSUE_PREFIX
            id.startsWith(CHAT_PREFIX) -> Kind.Chat to CHAT_PREFIX
            else -> return null
        }
        return id.removePrefix(prefix).takeIf { it.isNotEmpty() }?.let { Target(kind, it) }
    }
}
