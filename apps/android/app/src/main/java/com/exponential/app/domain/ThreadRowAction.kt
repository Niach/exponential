package com.exponential.app.domain

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

// #EXP-1464: what a conversation's SESSION rows do when tapped ×4 (a thread's
// Conversation and a team agent's conversation alike). A session card whose
// session waits on a question or plan EXPANDS in place into that card; every
// other row naming a session (a session card, a `Report from` line, an event
// line such as `… was not picked up`) OPENS it by inbox-route.json
// `sessionTarget`. Hand-ported from
// `packages/domain-contract/src/thread-row-action.ts`, locked to
// `fixtures/thread-row-action.json` by ThreadRowActionTest.

object ThreadRowAction {
    object Copy {
        const val OPEN_CHAT = "Open chat"
        const val ANSWERED = "Answered"
    }

    object TestIds {
        const val OPEN_CHAT = "thread-open-chat"
        const val EXPANDED = "thread-row-expanded"
        const val EVENT = "thread-event"
    }

    enum class Kind(val wire: String) { Expand("expand"), Open("open"), None("none") }

    data class Action(
        val action: Kind,
        val sessionId: String?,
        /** Only a row that does something carries the chevron (and press). */
        val chevron: Boolean,
        /** Overrides the card's state word ([Copy.ANSWERED]), null = its own. */
        val kindWord: String?,
        /** Overrides the card's tone with the word, null = its own. */
        val tone: String?,
    )

    private val OPENING_TYPES = setOf("session", "report", "event")

    val NONE = Action(Kind.None, null, chevron = false, kindWord = null, tone = null)

    /**
     * [type] = the Conversation row's type; [sessionId] = the session it names
     * (null = none / not synced); [rootId] = the conversation's own root;
     * [state] = that session's card state (null = unknown); [blocked] = a
     * flag stands behind its `needs_you`; [inline] = its waiting card already
     * is a row of this conversation ([waitingCardInline]: it opens instead of
     * expanding); [answered] = the viewer answered
     * the card this row expanded (client-held).
     */
    fun threadRowAction(
        type: String,
        sessionId: String?,
        rootId: String,
        state: SessionTree.SessionState?,
        blocked: Boolean,
        inline: Boolean,
        answered: Boolean,
    ): Action {
        if (sessionId.isNullOrEmpty() || sessionId == rootId || type !in OPENING_TYPES) return NONE
        if (type == "session" && state == SessionTree.SessionState.NeedsYou && !blocked && !inline) {
            return Action(Kind.Expand, sessionId, chevron = true, kindWord = null, tone = null)
        }
        val quiet = type == "session" && answered && state != null &&
            state != SessionTree.SessionState.NeedsYou && state != SessionTree.SessionState.Done
        return Action(
            Kind.Open,
            sessionId,
            chevron = true,
            kindWord = if (quiet) Copy.ANSWERED else null,
            tone = if (quiet) "muted" else null,
        )
    }

    /** Whether [sessionId]'s waiting card already is a row of [rows] (an
     *  unanswered question row or an undecided plan row of that session). */
    fun waitingCardInline(rows: List<SessionTree.ThreadRow>, sessionId: String): Boolean = rows.any { row ->
        when (row) {
            is SessionTree.ThreadRow.Question -> row.sessionId == sessionId && !row.answered
            is SessionTree.ThreadRow.Plan -> row.sessionId == sessionId && row.decision == null
            else -> false
        }
    }

    /** The card an expanding row opens: the session's NEWEST waiting
     *  `question` (answeredAt null) or `plan` (no decision) message id, else null. */
    fun sessionWaitingCard(sessionId: String, messages: List<SessionTree.Message>): String? {
        var bestId: String? = null
        var bestAt = Long.MIN_VALUE
        for (message in messages) {
            if (message.sessionId != sessionId) continue
            val card = message.card ?: continue
            val kind = (card["kind"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            val waiting = when (kind) {
                "question" -> message.answeredAt == null
                "plan" -> card["decision"].let { it == null || it is JsonNull }
                else -> false
            }
            if (!waiting) continue
            val at = WireTimestamps.parseEpochMs(message.createdAt) ?: continue
            if (bestId == null || at > bestAt || (at == bestAt && message.id > bestId)) {
                bestId = message.id
                bestAt = at
            }
        }
        return bestId
    }
}
