package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/fixtures/needs-you.json`,
 * TS reference `src/needs-you.ts`, iOS ExpCore `NeedsYou.swift`, desktop
 * `domain::needs_you`): every plan and question waiting on the viewer, plus
 * (#EXP-1699) the PRs ready to merge. `NeedsYouTest` replays every fixture case,
 * comparing the WHOLE expected array.
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

    /** The neutral copy of a mirror's origin the viewer may not read. */
    object Private {
        const val PLAN = "Plan in a private chat"
        const val QUESTION = "Question in a private chat"
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
        /** #EXP-1699: the session works now (a PR row waits on a person only when none does). */
        val busy: Boolean = false,
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

    /** #EXP-1699: the pull requests shape's row, as far as the fold reads it. */
    data class PullRequestInput(
        val id: String,
        val rootSessionId: String?,
        val number: Long,
        val title: String,
        val state: String,
        val checksState: String?,
        val basePullRequestId: String?,
        val createdAt: String,
        /** #EXP-1710: contract prMergeState (null = a row from before the train). */
        val mergeState: String? = null,
        /** #EXP-1710: `person` | `bounces` | `timeout`. */
        val holdReason: String? = null,
        /** #EXP-1710: when the opener session's device went offline (wire
         *  timestamp), null while online or unknown. */
        val deviceOfflineSince: String? = null,
    )

    // ── output ─────────────────────────────────────────────────────────────

    enum class Kind(val wire: String) { Plan("plan"), Question("question"), Pr("pr") }

    /** `Plan` = the chat's Plan tab; `Card` = its card in the Conversation;
     *  `Results` = its Results tab; `Chat` = an agent conversation. */
    enum class Opens(val wire: String) { Plan("plan"), Card("card"), Results("results"), Chat("chat") }

    data class Row(
        /** The waiting card's message (the origin's for a mirror it may read), or the PR's id. */
        val messageId: String,
        val sessionId: String,
        val rootId: String,
        val kind: Kind,
        val opens: Opens,
        val title: String,
        val chat: String,
        val meta: String,
        val age: String,
        val createdAt: String,
        /** #EXP-1710: a stuck PR's actions (THE Button sm row); empty otherwise. */
        val actions: List<MergeTrain.StuckAction> = emptyList(),
        /** #EXP-1710: the PR a [Kind.Pr] row is about (its actions' target). */
        val pullRequestNumber: Long? = null,
    )

    // ── the fold ───────────────────────────────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun ms(iso: String): Long = WireTimestamps.parseEpochMs(iso) ?: 0L

    private fun rowOf(
        message: MessageInput,
        sessions: Map<String, SessionInput>,
        /** Every session the viewer may read, ended ones included. */
        readable: Map<String, SessionInput>,
        now: Long,
    ): Row? {
        val card = message.card ?: return null
        val session = sessions[message.sessionId] ?: return null
        val root = sessions[session.rootId] ?: return null
        val age = ResultsTab.age(ms(message.createdAt), now)
        val kind = card.str("kind")
        if (kind == "plan" || kind == "question") {
            // A digest's card is cut (no `decision`): `answeredAt` settles it.
            if (message.answeredAt != null || WaitingCards.cardState(card, message.answeredAt) != WaitingCards.CardState.Waiting) return null
            val plan = kind == "plan"
            return Row(
                messageId = message.id,
                sessionId = session.id,
                rootId = root.id,
                kind = if (plan) Kind.Plan else Kind.Question,
                opens = if (plan) Opens.Plan else Opens.Card,
                title = if (plan) {
                    PlanPage.planRowLabel(card.str("plan") ?: "")
                } else {
                    AskDock.askDockTitle(card.str("question") ?: "").ifEmpty { AskDock.label(AskDock.Kind.Question) }
                },
                chat = root.name,
                meta = "",
                age = age,
                createdAt = message.createdAt,
            )
        }
        if (kind != "ask" || message.answeredAt != null) return null
        // A mirror: the origin's own ids where the viewer may read it, else the
        // mirror's, with the neutral copy (chats.json `askPreview`). A digest from
        // before it kept `messageId` opens the origin's chat, no card to reach.
        val originSession = (card.str("sessionId") ?: message.authorSessionId)?.let { readable[it] }
        val preview = Chats.askPreview(
            SessionTree.Message(
                id = message.id, sessionId = message.sessionId, authorKind = "agent", authorId = null,
                body = message.body, card = card, answeredAt = message.answeredAt, consumedAt = null,
                createdAt = message.createdAt, authorSessionId = message.authorSessionId,
            ),
            readable.keys,
        )
        val opaque = preview.text == Private.PLAN || preview.text == Private.QUESTION
        val target = if (opaque) null else originSession
        val named = if (target == null) null else card.str("messageId")
        val plan = preview.kind == "plan"
        return Row(
            messageId = named ?: message.id,
            sessionId = target?.id ?: session.id,
            rootId = target?.rootId ?: root.id,
            kind = if (plan) Kind.Plan else Kind.Question,
            opens = if (plan && named != null) Opens.Plan else Opens.Card,
            title = preview.text.removePrefix(Prefix.QUESTION),
            chat = target?.let { readable[it.rootId]?.name } ?: root.name,
            meta = "",
            age = age,
            createdAt = message.createdAt,
        )
    }

    /** #EXP-1710 (merge-train.json NEEDS YOU, `stuckRow`): a live PR the
     *  train gave up on, one row titled `Auto-merge stuck on #N` with its
     *  actions (Retry, Merge now for owners, Close PR); opens Results. */
    private fun stuckRows(
        now: Long,
        me: String?,
        owner: Boolean,
        sessions: List<SessionInput>,
        pullRequests: List<PullRequestInput>,
    ): List<Row> {
        val byId = sessions.associateBy { it.id }
        return pullRequests.mapNotNull { pr ->
            val root = pr.rootSessionId?.let { byId[it] } ?: return@mapNotNull null
            if (pr.state != "open" && pr.state != "draft") return@mapNotNull null
            if (root.agentId != null && root.userId != me) return@mapNotNull null
            val stuck = MergeTrain.stuckRow(
                number = pr.number,
                state = pr.mergeState ?: return@mapNotNull null,
                holdReason = pr.holdReason,
                deviceOfflineSince = pr.deviceOfflineSince,
                nowMs = now,
                owner = owner,
            ) ?: return@mapNotNull null
            Row(
                messageId = pr.id,
                sessionId = root.id,
                rootId = root.id,
                kind = Kind.Pr,
                opens = if (root.agentId == null) Opens.Results else Opens.Chat,
                title = stuck.title,
                chat = root.name,
                meta = stuck.meta,
                age = ResultsTab.age(ms(pr.createdAt), now),
                createdAt = pr.createdAt,
                actions = stuck.actions,
                pullRequestNumber = pr.number,
            )
        }
    }

    /** #EXP-1699: the PRs ready to merge (`prCases`). #EXP-1710: with the
     *  merge train they hold only in `approval` mode, never for a PR the
     *  train already took (queued, testing, merged). */
    private fun pullRequestRows(
        now: Long,
        me: String?,
        sessions: List<SessionInput>,
        pullRequests: List<PullRequestInput>,
    ): List<Row> {
        val byId = sessions.associateBy { it.id }
        val live = pullRequests.filter { it.state == "open" || it.state == "draft" }.map { it.id }.toSet()
        val working = sessions.filter { it.busy }.map { it.rootId }.toSet()
        return pullRequests.mapNotNull { pr ->
            val root = pr.rootSessionId?.let { byId[it] } ?: return@mapNotNull null
            if (pr.state != "open" || pr.checksState != "success") return@mapNotNull null
            if (pr.mergeState in TRAIN_TOOK) return@mapNotNull null
            if (root.agentId != null && root.userId != me) return@mapNotNull null
            if (root.id in working || (pr.basePullRequestId != null && pr.basePullRequestId in live)) return@mapNotNull null
            Row(
                messageId = pr.id,
                sessionId = root.id,
                rootId = root.id,
                kind = Kind.Pr,
                opens = if (root.agentId == null) Opens.Results else Opens.Chat,
                title = pr.title,
                chat = root.name,
                meta = "#${pr.number}",
                age = ResultsTab.age(ms(pr.createdAt), now),
                createdAt = pr.createdAt,
                pullRequestNumber = pr.number,
            )
        }
    }

    private val TRAIN_TOOK = setOf("queued", "testing", "merged")

    /** THE fold: one row per waiting card, once, plus (#EXP-1699) the PRs ready
     *  to merge, newest first. */
    fun needsYou(
        now: Long,
        me: String?,
        sessions: List<SessionInput>,
        messages: List<MessageInput>,
        pullRequests: List<PullRequestInput> = emptyList(),
        /**
         * #EXP-1710: the team's contract teamMergeMode. `auto` = no PR waits
         * for a person (the EXP-1699 ready rows go), `approval` = they stay.
         * The default `approval` keeps the pre-train fixture cases (needs-you.json
         * `prCases` carry no `mergeMode` yet).
         */
        mergeMode: String = MergeTrain.MODE_APPROVAL,
        /** #EXP-1710: the viewer owns the team (a stuck row's Merge now). */
        owner: Boolean = false,
    ): List<Row> {
        // Another member's agent conversation is theirs (chats.json `listAgents`).
        val readable = sessions.filter { it.agentId == null || it.userId == me }.associateBy { it.id }
        val live = readable.filterValues { it.status != "ended" }
        val rows = LinkedHashMap<String, Row>()
        val seen = HashSet<String>()
        val unnamed = mutableListOf<Row>()
        for (message in messages) {
            if (!seen.add(message.id)) continue
            val row = rowOf(message, live, readable, now) ?: continue
            if (message.id == row.messageId && row.sessionId != message.sessionId) {
                // An older digest's mirror naming only a session waits until every card is known.
                unnamed += row
            } else if (message.id == row.messageId || row.messageId !in rows) {
                // The card itself beats its mirror's copy of it.
                rows[row.messageId] = row
            }
        }
        val waitingIn = rows.values.map { it.sessionId }.toMutableSet()
        for (row in unnamed) {
            if (waitingIn.add(row.sessionId)) rows[row.messageId] = row
        }
        val stuck = stuckRows(now, me, owner, sessions, pullRequests)
        val stuckIds = stuck.mapTo(HashSet()) { it.messageId }
        val ready = if (mergeMode == MergeTrain.MODE_AUTO) {
            emptyList()
        } else {
            pullRequestRows(now, me, sessions, pullRequests).filter { it.messageId !in stuckIds }
        }
        return (rows.values + stuck + ready)
            .sortedWith(compareBy<Row> { it.kind == Kind.Pr }.thenByDescending { ms(it.createdAt) }.thenByDescending { it.messageId })
    }
}
