package com.exponential.app.domain

/**
 * #EXP-1640: App Actions (Gemini / Assistant), the Android mirror of
 * `packages/domain-contract/src/app-intents.ts`, locked by
 * `fixtures/app-intents.json` (`AppIntentsTest`). The `<capability>`s live in
 * `res/xml/shortcuts.xml`; MainActivity hands an invocation to
 * `data/intents/AppIntentRunner`.
 */
object AppIntents {
    const val CREATED = "Created {identifier}"
    const val ASK_TITLE = "What's the issue?"
    const val NO_BOARD = "Open Exponential and create a board first"
    const val NO_AGENT = "No agent called {name}"
    const val SENT = "Sent to {agent}"
    const val SIGNED_OUT = "Sign in to Exponential first"
    const val CREATE_ISSUE_TITLE = "Create issue"
    const val ASK_AGENT_TITLE = "Ask agent"

    const val CAPABILITY_CREATE_ISSUE = "actions.intent.CREATE_THING"
    const val CAPABILITY_ASK_AGENT = "actions.intent.CREATE_MESSAGE"

    const val TITLE_MAX = 256

    data class Board(val id: String, val sortOrder: Double)

    data class Agent(val id: String, val name: String, val sortOrder: Int, val role: String?, val ownerId: String?)

    private val WHITESPACE = Regex("\\s+")

    /** A spoken or typed issue title, null = ask again. */
    fun intentIssueTitle(text: String): String? {
        val title = text.replace(WHITESPACE, " ").trim()
        if (title.isEmpty()) return null
        val count = title.codePointCount(0, title.length)
        return if (count <= TITLE_MAX) title else title.substring(0, title.offsetByCodePoints(0, TITLE_MAX))
    }

    /** The board a new issue lands on, null = no board. */
    fun intentBoard(boards: List<Board>, lastBoardId: String?, pickedId: String?): String? {
        val ids = boards.map { it.id }.toSet()
        if (pickedId != null && pickedId in ids) return pickedId
        if (lastBoardId != null && lastBoardId in ids) return lastBoardId
        return boards.sortedBy { it.sortOrder }.firstOrNull()?.id
    }

    /** The agent a spoken name means: exact, then prefix, in agent order. */
    fun matchIntentAgent(agents: List<Agent>, me: String?, spoken: String): String? {
        val ordered = AgentSettings.Assistant.orderedAgents(agents, me) {
            AgentSettings.Assistant.OrderLike(it.role, it.ownerId, it.name, it.sortOrder)
        }
        val wanted = spoken.trim().lowercase()
        if (wanted.isEmpty()) return ordered.firstOrNull()?.id
        ordered.firstOrNull { it.name.lowercase() == wanted }?.let { return it.id }
        return ordered.firstOrNull { it.name.lowercase().startsWith(wanted) }?.id
    }

    fun created(identifier: String): String = CREATED.replace("{identifier}", identifier)
    fun noAgent(name: String): String = NO_AGENT.replace("{name}", name)
    fun sent(agent: String): String = SENT.replace("{agent}", agent)
}
