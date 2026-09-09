package com.exponential.app.domain

/**
 * EXP-788: the composer IS the free-text path of a pending card. While a plan
 * approval or a question waits on the human, a message typed into the
 * composer answers THAT card instead of starting a new turn — the in-card text
 * field is gone on every client. This is the ONE decision behind it, shared by
 * the send path (which frame goes out) and the composer's placeholder (what
 * the field says it will do).
 */
data class ComposerAnswerTarget(
    val question: AgentFeedItem.Question,
    /** The option keys the typed reply rides on in the `answer` frame. */
    val keys: List<String>,
) {
    /** Byte-identical ×4: what the composer promises while this card waits. */
    val placeholder: String
        get() = if (question.planMode) PLAN_PENDING_PLACEHOLDER else QUESTION_PENDING_PLACEHOLDER
}

/** Web copy, byte-matched: a plan is waiting and the field is the feedback path. */
const val PLAN_PENDING_PLACEHOLDER = "Tell the agent what to change, or pick an option above"

/** Web copy, byte-matched: a question is waiting and the field answers it. */
const val QUESTION_PENDING_PLACEHOLDER = "Answer directly, or pick an option above"

/**
 * The card a typed message answers, or null when the composer should send a
 * plain steer message.
 *
 * - Only an ANSWERABLE card qualifies: it carries a wire id, is unresolved and
 *   its answer lock does not hold (a Failed lock re-opens it, EXP-334). An
 *   ask's submit step consumes no answer, so it never qualifies either.
 * - The NEWEST such card wins; inside a multi-step ask the FIRST still-open
 *   step is the one showing, so that step takes the reply.
 * - A plan card answers on its reject option — the LAST option since EXP-788
 *   ("No, keep planning", whose description says the next message goes back
 *   to planning), with the typed text riding `text`.
 * - A question answers on its `freeText` option; a card without one has no
 *   free-text path at all, so the message stays a plain steer.
 * - A `/` command is never an answer: the composer's menu owns that text.
 */
fun composerAnswerTarget(
    feed: List<AgentFeedItem>,
    answerLocks: Map<String, AnswerState>,
    text: String = "",
): ComposerAnswerTarget? {
    if (text.trimStart().startsWith("/")) return null
    val open = feed.filterIsInstance<AgentFeedItem.Question>().filter { card ->
        val wireId = card.wireId ?: return@filter false
        !card.resolved && !card.isSubmitStep && !answerLocks[wireId].locksCard()
    }
    val newest = open.lastOrNull() ?: return null
    val card = newest.askId?.let { askId -> open.firstOrNull { it.askId == askId } } ?: newest
    val keys = when {
        card.planMode -> listOfNotNull(card.options.lastOrNull()?.key)
        else -> listOfNotNull(card.options.firstOrNull { it.freeText }?.key)
    }
    if (keys.isEmpty()) return null
    return ComposerAnswerTarget(card, keys)
}
