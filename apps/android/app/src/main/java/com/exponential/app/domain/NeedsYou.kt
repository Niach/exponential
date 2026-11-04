package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/fixtures/needs-you.json`,
 * TS reference `src/needs-you.ts`, iOS ExpCore `NeedsYou.swift`, desktop
 * `domain::needs_you`): every plan and question waiting on the viewer.
 * `NeedsYouTest` replays every fixture case, comparing the WHOLE expected array.
 */
object NeedsYou {

    const val ROUTE = "/t/\$teamSlug/needs-you"

    object Copy {
        const val ENTRY = "Needs you"
        const val EMPTY = "Nothing needs you"
    }

    object Prefix {
        const val QUESTION = "Question: "
    }

    object TestIds {
        const val ENTRY = "nav-needs-you"
        const val ROW = "needs-you-row"
        const val LIST = "needs-you"
    }

    // ── input ──────────────────────────────────────────────────────────────

    data class SessionInput(
        val id: String,
        val rootId: String,
        /** The session's display name (session-title; an agent conversation = the agent's). */
        val name: String,
        val status: String,
        val agentId: String?,
        val userId: String?,
    )

    data class MessageInput(
        val id: String,
        val sessionId: String,
        val authorSessionId: String?,
        val body: String,
        /** Wire timestamps, as the digests carry them. */
        val createdAt: String,
        val answeredAt: String?,
        val card: JsonObject?,
    )

    // ── output ─────────────────────────────────────────────────────────────

    /** `Plan` = the chat's Plan tab; `Card` = its card in the Conversation. */
    enum class Opens(val wire: String) { Plan("plan"), Card("card") }

    data class Row(
        /** The waiting card's message: the origin's for a mirror it may read. */
        val messageId: String,
        val sessionId: String,
        val rootId: String,
        val kind: AskDock.Kind,
        val opens: Opens,
        val title: String,
        val chat: String,
        val meta: String,
        val age: String,
        val createdAt: String,
    )

    // ── the fold ───────────────────────────────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun ms(iso: String): Long = WireTimestamps.parseEpochMs(iso) ?: 0L

    private fun rowOf(message: MessageInput, sessions: Map<String, SessionInput>, now: Long): Row? {
        val card = message.card ?: return null
        val session = sessions[message.sessionId] ?: return null
        val root = sessions[session.rootId] ?: return null
        val age = ResultsTab.age(ms(message.createdAt), now)
        val kind = card.str("kind")
        if (kind == "plan" || kind == "question") {
            if (WaitingCards.cardState(card, message.answeredAt) != WaitingCards.CardState.Waiting) return null
            val plan = kind == "plan"
            return Row(
                messageId = message.id,
                sessionId = session.id,
                rootId = root.id,
                kind = if (plan) AskDock.Kind.Plan else AskDock.Kind.Question,
                opens = if (plan) Opens.Plan else Opens.Card,
                title = if (plan) {
                    PlanPage.planRowLabel(card.str("plan") ?: "")
                } else {
                    AskDock.askDockTitle(card.str("question") ?: "").ifEmpty { AskDock.label(AskDock.Kind.Question) }
                },
                chat = root.name,
                meta = if (plan) PlanPage.countsLine(PlanPage.counts(card)) else "",
                age = age,
                createdAt = message.createdAt,
            )
        }
        if (kind != "ask" || message.answeredAt != null) return null
        // A mirror: the origin's own ids where the viewer may read it, else the
        // mirror's, with the neutral copy (chats.json `askPreview`).
        val originSession = (card.str("sessionId") ?: message.authorSessionId)?.let { sessions[it] }
        val preview = Chats.askPreview(
            SessionTree.Message(
                id = message.id, sessionId = message.sessionId, authorKind = "agent", authorId = null,
                body = message.body, card = card, answeredAt = message.answeredAt, consumedAt = null,
                createdAt = message.createdAt, authorSessionId = message.authorSessionId,
            ),
            sessions.keys,
        )
        val plan = preview.kind == "plan"
        val rowKind = if (plan) AskDock.Kind.Plan else AskDock.Kind.Question
        val title = preview.text.removePrefix(Prefix.QUESTION)
        val originMessage = card.str("messageId")
        if (originSession != null && originMessage != null) {
            return Row(
                messageId = originMessage,
                sessionId = originSession.id,
                rootId = originSession.rootId,
                kind = rowKind,
                opens = if (plan) Opens.Plan else Opens.Card,
                title = title,
                chat = sessions[originSession.rootId]?.name ?: root.name,
                meta = "",
                age = age,
                createdAt = message.createdAt,
            )
        }
        return Row(
            messageId = message.id,
            sessionId = session.id,
            rootId = root.id,
            kind = rowKind,
            opens = Opens.Card,
            title = title,
            chat = root.name,
            meta = "",
            age = age,
            createdAt = message.createdAt,
        )
    }

    /** THE fold: one row per waiting card, once, newest first. */
    fun needsYou(
        now: Long,
        me: String?,
        sessions: List<SessionInput>,
        messages: List<MessageInput>,
    ): List<Row> {
        // Another member's agent conversation is theirs (chats.json `listAgents`).
        val live = sessions
            .filter { it.status != "ended" && (it.agentId == null || it.userId == me) }
            .associateBy { it.id }
        val rows = LinkedHashMap<String, Row>()
        val seen = HashSet<String>()
        for (message in messages) {
            if (!seen.add(message.id)) continue
            val row = rowOf(message, live, now) ?: continue
            // The card itself beats its mirror's copy of it.
            if (message.id == row.messageId || row.messageId !in rows) rows[row.messageId] = row
        }
        return rows.values.sortedWith(compareByDescending<Row> { ms(it.createdAt) }.thenByDescending { it.messageId })
    }
}
