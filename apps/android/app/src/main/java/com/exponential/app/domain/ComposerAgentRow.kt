package com.exponential.app.domain

/**
 * S-3 (owner 2026-10-28): the New-chat composer's AGENT ROW, the Android
 * mirror of `packages/domain-contract/src/composer-agent-row.ts`, locked by
 * `fixtures/composer-home.json` `agentRow.cases` (`ComposerAgentRowTest`).
 * Circles = the viewer's Assistant first (brand mark), then the team agents
 * they see, then the dotted Temporary session circle.
 */
object ComposerAgentRow {
    const val SIZE = 40
    const val RING_WIDTH = 2
    const val RING_GAP = 3
    const val GAP = 10
    const val LABEL_TO_ROW = 8
    const val ROW_TO_CARD = 12
    const val TEMPORARY_ICON = "ui-icon-placeholder"
    /** The dotted circle's id (Temporary session = a normal session). */
    const val TEMPORARY_PICK = "temporary"
    const val COPY_TO = "To {name}"
    const val COPY_TEMPORARY = "Temporary session"
    const val TEST_ID_ROW = "composer-agent-row"
    const val TEST_ID_LABEL = "composer-agent-label"

    fun optionTestId(id: String): String = "composer-agent-$id"

    data class Agent(
        val id: String,
        val role: String?,
        val ownerId: String?,
        val name: String,
        val sortOrder: Int,
    )

    data class State<T>(
        /** The circles in order: the agents (the dotted circle follows). */
        val agents: List<T>,
        /** Option ids in order, [TEMPORARY_PICK] last. */
        val options: List<String>,
        /** The selected option id (never empty). */
        val selected: String,
        /** The picked agent, null = Temporary session. */
        val agent: T?,
        /** The label line over the circles. */
        val label: String,
    )

    /** The row for a viewer: [picked] = the last tap (null/unknown = the default). */
    fun <T> composerAgentRow(agents: List<T>, me: String?, picked: String?, row: (T) -> Agent): State<T> {
        val ordered = AgentSettings.Assistant.orderedAgents(agents, me) {
            row(it).let { a -> AgentSettings.Assistant.OrderLike(a.role, a.ownerId, a.name, a.sortOrder) }
        }
        val options = ordered.map { row(it).id } + TEMPORARY_PICK
        val fallback = ordered.firstOrNull { AgentSettings.Assistant.isAssistant(row(it).role) }?.let { row(it).id }
            ?: TEMPORARY_PICK
        val selected = if (!picked.isNullOrEmpty() && picked in options) picked else fallback
        val agent = ordered.firstOrNull { row(it).id == selected }
        val label = agent?.let { COPY_TO.replace("{name}", row(it).name) } ?: COPY_TEMPORARY
        return State(ordered, options, selected, agent, label)
    }
}
