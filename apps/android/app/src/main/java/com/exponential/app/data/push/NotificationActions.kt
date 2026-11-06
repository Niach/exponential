package com.exponential.app.data.push

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EXP-1641: answering from the notification, the Android mirror of
 * `packages/domain-contract/src/notification-actions.ts`, locked by
 * `fixtures/notification-actions.json` (`NotificationActionsTest`). The
 * fixture's `$comment` holds the rules: which push offers which actions, in
 * which order. FcmService draws them; [NotificationActionReceiver] runs them.
 */
object NotificationActions {
    const val APPROVE_LABEL = "Approve"
    const val REQUEST_CHANGES_LABEL = "Request changes…"
    const val REQUEST_CHANGES_PLACEHOLDER = "What should change?"
    const val ANSWER_LABEL = "Answer…"
    const val ANSWER_PLACEHOLDER = "Answer"
    const val REPLY_LABEL = "Reply"
    const val REPLY_PLACEHOLDER = "Reply"
    const val SEND_LABEL = "Send"
    const val OPEN_LABEL = "Open"
    const val FAILED = "Couldn't send. Open Exponential to retry."

    const val KEY_MESSAGE_ID = "messageId"
    const val KEY_CARD_KIND = "cardKind"
    const val KEY_OPTIONS = "options"
    const val KEY_CUSTOM = "custom"
    const val KEY_PLAN_QUESTIONS = "planQuestions"
    const val KEY_SEALED = "sealed"

    const val MAX_OPTION_BUTTONS = 3

    const val APPROVE = "approve"
    const val REQUEST_CHANGES = "request-changes"
    const val ANSWER = "answer"
    const val REPLY = "reply"
    const val OPEN = "open"
    const val OPTION_PREFIX = "option:"

    enum class Kind { Button, Text }

    data class Action(val id: String, val kind: Kind, val title: String)

    data class AskOptions(val options: List<String>, val custom: Boolean)

    private val FLAGS = Regex("""\s\(((?:several allowed|free text allowed)(?:, (?:several allowed|free text allowed))*)\)$""")
    private const val OPTIONS_LEAD = "Options: "
    private const val PLAN_QUESTIONS = "\n\nQuestions:\n- "

    /** A question's options and free-text flag, read from its bus ask text. */
    fun askTextOptions(askText: String): AskOptions {
        val line = askText.split("\n").drop(1).firstOrNull { it.startsWith(OPTIONS_LEAD) }
            ?: return AskOptions(emptyList(), custom = true)
        var rest = line.removePrefix(OPTIONS_LEAD)
        val flags = FLAGS.find(rest)
        if (flags != null) rest = rest.substring(0, flags.range.first)
        val options = rest.split(" | ").filter { it.isNotEmpty() }
        return AskOptions(options, custom = flags?.groupValues?.get(1)?.contains("free text allowed") == true)
    }

    private fun button(id: String, title: String) = Action(id, Kind.Button, title)
    private fun text(id: String, title: String) = Action(id, Kind.Text, title)

    private fun parseOptions(raw: String?): List<String> {
        raw ?: return emptyList()
        val parsed = runCatching { Json.parseToJsonElement(raw) }.getOrNull() as? JsonArray ?: return emptyList()
        return parsed.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
    }

    /**
     * The actions a delivered push offers, in order (empty = none).
     * [keyHeld] = this install holds the session tree's key; [openedAskText] =
     * a sealed card's ask text opened here (null = not opened).
     */
    fun notificationActions(data: Map<String, String>, keyHeld: Boolean, openedAskText: String?): List<Action> {
        val sealed = data[KEY_SEALED] == "1"
        val kind = data[KEY_CARD_KIND]
        val open = button(OPEN, OPEN_LABEL)
        if (kind == "plan" || kind == "question") {
            if (data[KEY_MESSAGE_ID].isNullOrEmpty()) return listOf(open)
            val opened = if (sealed && keyHeld) openedAskText else null
            if (sealed && opened == null) return listOf(open)
            if (kind == "plan") {
                val questions = opened?.contains(PLAN_QUESTIONS)
                    ?: ((data[KEY_PLAN_QUESTIONS] ?: "0").toDoubleOrNull()?.let { it > 0 } ?: false)
                val changes = text(REQUEST_CHANGES, REQUEST_CHANGES_LABEL)
                return if (questions) listOf(changes, open) else listOf(button(APPROVE, APPROVE_LABEL), changes, open)
            }
            val ask = opened?.let(::askTextOptions)
                ?: AskOptions(parseOptions(data[KEY_OPTIONS]), custom = data[KEY_CUSTOM] != "0")
            val room = if (ask.custom) MAX_OPTION_BUTTONS - 1 else MAX_OPTION_BUTTONS
            val buttons = if (ask.options.size <= room) {
                ask.options.mapIndexed { i, option -> button("$OPTION_PREFIX$i", option) }
            } else {
                emptyList()
            }
            val actions = if (ask.custom) buttons + text(ANSWER, ANSWER_LABEL) else buttons
            return actions.ifEmpty { listOf(open) }
        }
        if (!data["sessionId"].isNullOrEmpty()) {
            return if (sealed && !keyHeld) emptyList() else listOf(text(REPLY, REPLY_LABEL))
        }
        return emptyList()
    }

    /** How a tapped action runs (never in clear for a `sealed=1` push). */
    sealed interface Route {
        /** The composer's send ([SessionsApi.send]). */
        data object Send : Route
        /** Sealed for the named tree ([SessionsApi.sendSealed]), the session row not needed. */
        data class SendSealed(val rootId: String) : Route
        /** THE answer path ([SessionsApi.answer]), re-sealing a sealed card. */
        data object Answer : Route
        /** The card first, off the shape on demand, then [Answer]. */
        data object FetchThenAnswer : Route
        /** Nothing is sent: the failed notification. */
        data object Refuse : Route
    }

    /**
     * [actionId] of a push ([sealed] = `dataKeys.sealed` is `1`, [rootId] its
     * tree, [cardLocal] = the card's message row is in the local store).
     */
    fun route(actionId: String, sealed: Boolean, rootId: String?, cardLocal: Boolean): Route = when {
        actionId == REPLY -> when {
            !sealed -> Route.Send
            rootId.isNullOrEmpty() -> Route.Refuse
            else -> Route.SendSealed(rootId)
        }
        actionId == APPROVE || actionId == REQUEST_CHANGES || actionId == ANSWER ||
            actionId.startsWith(OPTION_PREFIX) ->
            if (sealed && !cardLocal) Route.FetchThenAnswer else Route.Answer
        else -> Route.Refuse
    }
}
