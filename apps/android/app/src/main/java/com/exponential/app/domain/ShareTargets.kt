package com.exponential.app.domain

/**
 * EXP-1635: sharing INTO Exponential, the Android mirror of
 * `packages/domain-contract/src/share-targets.ts`, locked by
 * `fixtures/share-targets.json` (`ShareTargetsTest`). The fixture's `$comment`
 * holds the rules: the Share to page's rows, the composer prefill, the system
 * share sheet's sharing shortcuts and the ids they carry.
 */
object ShareTargets {
    const val TITLE = "Share to"
    const val NEW_ISSUE_LABEL = "New issue"
    const val NEW_CHAT_LABEL = "New chat"
    const val NEW_CHAT_SUBTITLE = "Temporary session"
    const val AGENTS_LABEL = "Agents"
    const val IMAGES_ONE = "1 image"
    const val IMAGES_MANY = "{n} images"

    const val NEW_ISSUE = "new-issue"
    const val NEW_CHAT = "new-chat"
    const val NEW_ISSUE_ICON = "nav-create-issue"
    const val NEW_CHAT_ICON = "ui-icon-placeholder"

    const val AGENT_PREFIX = "share-agent:"
    const val CHAT_PREFIX = "share-chat:"
    const val MAX_ENTRIES = 4

    const val TEST_ID_PAGE = "share-targets"
    const val TEST_ID_PREVIEW = "share-targets-preview"

    fun rowTestId(id: String): String = "share-target-$id"

    /** The preview line's label; null = no line (a text-only share). */
    fun previewLabel(images: Int): String? = when (images) {
        0 -> null
        1 -> IMAGES_ONE
        else -> IMAGES_MANY.replace("{n}", images.toString())
    }

    data class Agent(
        val id: String,
        val name: String,
        val sortOrder: Int,
        val role: String?,
        val ownerId: String?,
    )

    data class Chat(
        val id: String,
        val title: String,
        val updatedAt: String,
        /** The agent a conversation session belongs to: never a recent chat row. */
        val agentId: String?,
        /** A tree key or a sealed title: never handed to the OS. */
        val sealed: Boolean = false,
    )

    data class Targets<T>(val rows: List<String>, val agents: List<T>)

    data class Entry(val id: String, val title: String)

    enum class Kind { Agent, Chat }

    data class DirectTarget(val kind: Kind, val id: String)

    /** The Share to page: the fixed rows, then the agents in agent row order. */
    fun <T> shareTargets(agents: List<T>, me: String?, row: (T) -> Agent): Targets<T> =
        Targets(listOf(NEW_ISSUE, NEW_CHAT), ordered(agents, me, row))

    /** What a shared payload leaves in the composer. */
    fun shareChatText(text: String?, subject: String?, url: String?, hasImages: Boolean): String {
        val trimmed = text?.trim().orEmpty()
        val link = url?.trim().orEmpty()
        val body = if (link.isNotEmpty() && !trimmed.contains(link)) {
            listOf(trimmed, link).filter { it.isNotEmpty() }.joinToString("\n\n")
        } else {
            trimmed
        }
        val lead = subject?.trim().orEmpty()
        if (hasImages || lead.isEmpty() || body.contains(lead)) return body
        return listOf(lead, body).filter { it.isNotEmpty() }.joinToString("\n\n")
    }

    /** The system share sheet's rows: agents, then recent chats, capped. */
    fun <T> directShareEntries(agents: List<T>, chats: List<Chat>, me: String?, row: (T) -> Agent): List<Entry> {
        val agentEntries = ordered(agents, me, row).map { row(it) }.map { Entry("$AGENT_PREFIX${it.id}", it.name) }
        val chatEntries = chats
            .filter { it.agentId.isNullOrEmpty() && !it.sealed }
            .sortedByDescending { it.updatedAt }
            .map { Entry("$CHAT_PREFIX${it.id}", it.title) }
        return (agentEntries + chatEntries).take(MAX_ENTRIES)
    }

    /** The destination a direct-share id names, null = fall back to step 1. */
    fun parseDirectShareId(id: String): DirectTarget? {
        val (kind, prefix) = when {
            id.startsWith(AGENT_PREFIX) -> Kind.Agent to AGENT_PREFIX
            id.startsWith(CHAT_PREFIX) -> Kind.Chat to CHAT_PREFIX
            else -> return null
        }
        val rest = id.removePrefix(prefix)
        return if (rest.isEmpty()) null else DirectTarget(kind, rest)
    }

    /** New issue's boards on the picked team (#EXP-1823), and the one it opens on. */
    data class IssueBoards<T>(val boards: List<T>, val board: T?)

    fun <T> shareIssueBoards(teamId: String?, boards: List<T>, lastBoardId: String?, id: (T) -> String, team: (T) -> String): IssueBoards<T> {
        val own = if (teamId == null) emptyList() else boards.filter { team(it) == teamId }
        return IssueBoards(own, own.firstOrNull { id(it) == lastBoardId } ?: own.firstOrNull())
    }

    private fun <T> ordered(agents: List<T>, me: String?, row: (T) -> Agent): List<T> =
        AgentSettings.Assistant.orderedAgents(agents, me) {
            row(it).let { a -> AgentSettings.Assistant.OrderLike(a.role, a.ownerId, a.name, a.sortOrder) }
        }
}
