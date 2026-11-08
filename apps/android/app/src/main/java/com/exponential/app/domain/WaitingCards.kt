package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EXP-1561 / #EXP-1562: WAITING question and plan cards ×4
 * (fixtures/waiting-cards.json; TS reference
 * `packages/domain-contract/src/waiting-cards.ts`, desktop
 * `domain::waiting_cards`, iOS ExpCore `WaitingCards.swift`): a card's state,
 * the dismiss ×, and the chat header's waiting chips. `WaitingCardsTest`
 * replays `stateCases` and `chipCases`, comparing whole values.
 */
object WaitingCards {

    object Copy {
        const val DISMISS = "Dismiss"
        const val DISMISSED = "Dismissed"
        const val WAITING = "Waiting on you"
        const val WAITING_COUNT = "{count} waiting"
    }

    /** Icon CONCEPT names (icons.json). */
    object Icons {
        const val PLAN = "coding-plan"
        const val QUESTION = "ui-help"
        const val DISMISS = "ui-close"
        const val WAITING = "ui-help"

        fun of(kind: AskDock.Kind): String = if (kind == AskDock.Kind.Plan) PLAN else QUESTION
    }

    object Constants {
        const val LABEL_MAX = 24
        const val ELLIPSIS = "…"
        const val HIGHLIGHT_MS = 1600L
    }

    object TestIds {
        const val DISMISS = "card-dismiss"
        const val DISMISSED = "card-dismissed"
        const val CHIP = "waiting-chip"
        const val BUTTON = "waiting-button"
        const val MENU = "waiting-menu"
    }

    enum class CardState(val wire: String) {
        Waiting("waiting"),
        Answered("answered"),
        Dismissed("dismissed"),
        Superseded("superseded"),
    }

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** `card.dismissed === true || card.decision === 'dismissed'`. */
    fun isDismissed(card: JsonObject): Boolean =
        (card["dismissed"] as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true" ||
            card.str("decision") == AskDock.DECISION_DISMISSED

    /** A question/plan card's state; null = not a question or plan. */
    fun cardState(card: JsonObject?, answeredAt: String?): CardState? {
        if (card == null) return null
        val kind = card.str("kind")
        if (kind != "plan" && kind != "question") return null
        if (isDismissed(card)) return CardState.Dismissed
        if (kind == "plan") {
            val decision = card.str("decision")
            if (decision == AskDock.DECISION_SUPERSEDED) return CardState.Superseded
            return if (decision != null) CardState.Answered else CardState.Waiting
        }
        return if (answeredAt != null || card.str("answer") != null) CardState.Answered else CardState.Waiting
    }

    /** One waiting card as the chips read it. */
    data class Input(val id: String, val kind: AskDock.Kind, val text: String)

    data class Chip(val id: String, val kind: AskDock.Kind, val icon: String, val label: String)

    enum class Layout(val wire: String) { Hidden("hidden"), Chips("chips"), Button("button") }

    data class Chips(val layout: Layout, val count: Int, val chips: List<Chip>)

    /** A title clipped to [Constants.LABEL_MAX] characters (code points). */
    fun clipLabel(title: String): String {
        val points = title.codePoints().toArray()
        if (points.size <= Constants.LABEL_MAX) return title
        val head = String(points, 0, Constants.LABEL_MAX - 1).trimEnd()
        return head + Constants.ELLIPSIS
    }

    /** The header's waiting chips (Conversation order kept): hidden when none
     *  waits, ONE button on phones, else one chip per card. */
    fun waitingChips(cards: List<Input>, phone: Boolean): Chips {
        val chips = cards.map { card ->
            Chip(
                id = card.id,
                kind = card.kind,
                icon = Icons.of(card.kind),
                label = clipLabel(AskDock.askDockTitle(card.text)).ifEmpty { AskDock.label(card.kind) },
            )
        }
        val layout = when {
            chips.isEmpty() -> Layout.Hidden
            phone -> Layout.Button
            else -> Layout.Chips
        }
        return Chips(layout, chips.size, chips)
    }

    /** A chat's chip inputs: ask-dock `chatPendingAsks` (Conversation order). */
    fun inputs(pending: List<AskDock.Pending>): List<Input> = pending.map { Input(it.id, it.kind, it.text) }

    /** The phone button's accessible name. */
    fun waitingCountLabel(count: Int): String = Copy.WAITING_COUNT.replace("{count}", count.toString())
}
