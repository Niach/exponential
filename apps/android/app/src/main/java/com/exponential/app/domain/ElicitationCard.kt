package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

/**
 * #NEW-146 (fixtures/elicitation-card.json): THE elicitation card ×4, the
 * question card a THIRD-PARTY MCP server's elicitation becomes. The device
 * builds the card (the form or the URL review as its surface) and maps the
 * answer back; a renderer only needs [Sidecar] (who asks, which mode, the
 * required fields, the url), [missing] (Submit disabled while any) and
 * [action] (what a pick means). Hand-mirrored from the TS reference
 * `@exp/domain-contract/elicitation-card`; `ElicitationCardTest` replays
 * `missingCases` + `actionCases`.
 */
object ElicitationCard {
    /** The card's `elicitation` sidecar (session-tree.ts `elicitationField`). */
    data class Sidecar(
        /** The configured server's name: the card's label, always. */
        val server: String,
        val mode: Mode,
        /** The form's required property names (empty in url mode). */
        val required: List<String>,
        /** The url to open (url mode); null otherwise. */
        val url: String?,
    )

    enum class Mode(val wire: String) { Form("form"), Url("url") }

    enum class Action(val wire: String) { Accept("accept"), Decline("decline"), Cancel("cancel") }

    /** `formKey`: the form's values live under `/form/<name>`. */
    const val FORM_KEY = "form"

    object Copy {
        const val SUBMIT = "Submit"
        const val DECLINE = "Decline"
        const val CANCEL = "Cancel"
        const val OPEN_LINK = "Open link"
        const val DONE = "I'm done"
        const val MISSING = "Fill in every required field."
        const val UNKNOWN_SERVER = "MCP server"

        /** `copy.specimen`: the specimen's four captions. */
        object Specimen {
            const val FORM = "Form"
            const val URL = "URL"
            const val WARNED = "URL with warnings"
            const val ANSWERED = "Answered"
        }
    }

    object TestIds {
        const val CARD = "elicitation-card"
        const val SERVER = "elicitation-server"
        const val SUBMIT = "elicitation-submit"
        const val DECLINE = "elicitation-decline"
        const val CANCEL = "elicitation-cancel"
        const val OPEN_LINK = "elicitation-open-link"
        const val DONE = "elicitation-done"
    }

    /** A card's sidecar when it is a well-formed one (else null). */
    fun sidecar(value: JsonElement?): Sidecar? {
        val v = value as? JsonObject ?: return null
        val server = (v["server"] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return null
        val mode = when ((v["mode"] as? JsonPrimitive)?.takeIf { it.isString }?.content) {
            Mode.Form.wire -> Mode.Form
            Mode.Url.wire -> Mode.Url
            else -> return null
        }
        val required = (v["required"] as? JsonArray)
            ?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
            .orEmpty()
        val url = (v["url"] as? JsonPrimitive)?.takeIf { it.isString }?.content
        return Sidecar(server, mode, required, url)
    }

    /** A value counts as given: a non-blank string, a finite number, any
     *  boolean, a non-empty list. */
    private fun given(value: JsonElement?): Boolean = when (value) {
        null, is JsonNull -> false
        is JsonArray -> value.isNotEmpty()
        is JsonObject -> false
        is JsonPrimitive -> when {
            value.isString -> value.content.isNotBlank()
            value.booleanOrNull != null -> true
            else -> value.doubleOrNull?.isFinite() == true
        }
    }

    /** The required fields still empty (Submit is disabled while any is,
     *  [Copy.MISSING] under the buttons). */
    fun missing(required: List<String>, surfaceData: JsonObject?): List<String> {
        val form = surfaceData?.get(FORM_KEY) as? JsonObject
        return required.filter { !given(form?.get(it)) }
    }

    /** Submit / I'm done = accept, Decline = decline, anything else (Cancel,
     *  the ×, a label never offered) = cancel. Never an accept by default. */
    fun action(answers: List<String>?, answer: String?, dismissed: Boolean?): Action {
        if (dismissed == true) return Action.Cancel
        val pick = (answers?.firstOrNull() ?: answer ?: "").trim()
        return when (pick) {
            Copy.SUBMIT, Copy.DONE -> Action.Accept
            Copy.DECLINE -> Action.Decline
            else -> Action.Cancel
        }
    }
}
