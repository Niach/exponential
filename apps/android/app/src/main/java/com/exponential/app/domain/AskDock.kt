package com.exponential.app.domain

/**
 * THE ask cards ×4 (fixtures/ask-dock.json; TS reference
 * `packages/domain-contract/src/ask-dock.ts`, desktop `domain::ask_dock`,
 * iOS ExpCore `AskDock.swift`): N-12: a question or plan card waiting on the
 * viewer is a DockCard of THE Dock ([Dock], fixtures/dock.json); the composer
 * never leaves. This file keeps the cards' own words: the label and title,
 * the plan options, a chat's waiting cards. `AskDockTest` replays the
 * fixture and compares whole objects.
 */
object AskDock {

    object Copy {
        const val PLAN_READY = "Plan ready"
        const val QUESTION = "Question"
        const val BACK = "Back to all cards"
        const val FEEDBACK_PLACEHOLDER = "Tell the agent what to change…"
        const val CANCEL = "Cancel"
        /** FEED-140: a waiting plan the agent replaced with a newer one. */
        const val SUPERSEDED = "Replaced by a newer plan"
        /** #EXP-1561: a card a person dismissed (waiting-cards.json). */
        const val DISMISSED = "Dismissed"
    }

    /** Icon CONCEPT names (icons.json). */
    object Icons {
        const val PLAN = "coding-plan"
        const val QUESTION = "ui-help"
    }

    enum class Kind(val wire: String) { Plan("plan"), Question("question") }

    data class Pending(
        /** The card's id (a `messages.id`, or the session view's ask id). */
        val id: String,
        val kind: Kind,
        /** The plan, or the question's text (markdown). */
        val text: String,
        /** The asking session's name in a chat; null in the session view. */
        val sessionName: String?,
        /** Order, oldest first. */
        val createdAt: Long,
        /** A plan's question count (a question: null). */
        val questions: Int? = null,
        /** #EXP-1785: the asking session (a chat row's; null in the session view). */
        val sessionId: String? = null,
    )

    data class PlanOption(
        val key: String,
        val label: String,
        val description: String?,
        /** `approved` | `rejected`. */
        val decision: String,
        val fresh: Boolean,
    )

    /** THE plan card's options ×4: Yes · Yes, and start with a fresh context
     *  · No, keep planning (the feedback field rides any decision). */
    val PLAN_OPTIONS: List<PlanOption> = listOf(
        PlanOption("approve", "Yes", null, "approved", false),
        PlanOption("approve-fresh", "Yes, and start with a fresh context", null, "approved", true),
        PlanOption("reject", "No, keep planning", "Your feedback goes back with it", "rejected", false),
    )

    /** FEED-140: the decision of a plan a newer `plan` call replaced. */
    const val DECISION_SUPERSEDED = "superseded"

    /** #EXP-1561: the decision of a plan a person dismissed (waiting-cards.json). */
    const val DECISION_DISMISSED = "dismissed"

    /** The option a decided plan card names as its answer. */
    fun planDecisionLabel(decision: String, fresh: Boolean): String {
        // FEED-140: a plan the agent replaced with a newer one.
        if (decision == DECISION_SUPERSEDED) return Copy.SUPERSEDED
        // #EXP-1561: a person dismissed it.
        if (decision == DECISION_DISMISSED) return Copy.DISMISSED
        val wantsFresh = decision == "approved" && fresh
        return PLAN_OPTIONS.firstOrNull { it.decision == decision && it.fresh == wantsFresh }?.label ?: ""
    }

    private val BLOCK_MARKER = Regex("""^(#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+|\[[ xX]\]\s+)""")
    private val LINK = Regex("""\[([^\]]*)\]\([^)]*\)""")

    /** The first plain line of a markdown body: fenced code skipped whole,
     *  block markers stripped, links reduced to their text, `**` / `__` /
     *  backticks dropped. */
    fun askDockTitle(text: String): String {
        var fenced = false
        for (raw in text.split("\n")) {
            var line = raw.trim()
            if (line.startsWith("```") || line.startsWith("~~~")) {
                fenced = !fenced
                continue
            }
            if (fenced) continue
            var stripped = BLOCK_MARKER.replaceFirst(line, "")
            while (stripped != line) {
                line = stripped
                stripped = BLOCK_MARKER.replaceFirst(line, "")
            }
            line = LINK.replace(line, "$1")
                .replace("**", "")
                .replace("__", "")
                .replace("`", "")
                .trim()
            if (line.isNotEmpty()) return line
        }
        return ""
    }

    fun label(kind: Kind): String = if (kind == Kind.Plan) Copy.PLAN_READY else Copy.QUESTION

    /** A chat's waiting cards, Conversation order: every plan row with no
     *  decision and every unanswered question row, of ANY session in the
     *  tree; [sessionName] names the asking session (null = unnamed);
     *  a plan carries its question count. */
    fun chatPendingAsks(
        rows: List<SessionTree.SessionChatRow>,
        sessionName: (String) -> String?,
    ): List<Pending> = rows.mapIndexedNotNull { index, row ->
        when {
            row is SessionTree.SessionChatRow.Plan && row.decision == null ->
                Pending(row.id, Kind.Plan, row.plan, sessionName(row.sessionId), index.toLong(), row.questions.size, row.sessionId)
            row is SessionTree.SessionChatRow.Question && !row.answered ->
                Pending(row.id, Kind.Question, row.question, sessionName(row.sessionId), index.toLong(), sessionId = row.sessionId)
            else -> null
        }
    }

    /** A chat's card captions: each session TAB's name (nested ones
     *  included); the root (no tab) = null. */
    fun chatSessionNames(tabs: List<SessionTree.ChatTab>): (String) -> String? {
        val names = tabs.filterIsInstance<SessionTree.ChatTab.SessionTab>().flatMap { tab ->
            listOf(tab.sessionId to tab.name) + tab.nested.map { it.sessionId to it.name }
        }.toMap()
        return { id -> names[id] }
    }

    // ── EXP-1398: THE session view's cards (ask-dock.json `sessionCardsCases`,
    // `anchorCases`) ────────────────────────────────────────────────────────

    enum class SessionCardKind(val wire: String, val postingCall: String) {
        Plan("plan", "session_self_plan"),
        Question("question", "session_self_ask"),
        Surface("surface", "session_self_show"),
    }

    /** One card of a session's own conversation: [waiting] = a plan with no
     *  decision / an unanswered question (it also docks). */
    data class SessionCard(val id: String, val kind: SessionCardKind, val waiting: Boolean)

    /** The session view's cards, oldest first (createdAt, then id): every
     *  `plan` / `question` card a TOOL posted (no `questionId`) and every
     *  `surface` card. Each draws inline; a waiting one also docks. */
    fun sessionCards(messages: List<SessionTree.Message>): List<SessionCard> = messages
        .mapNotNull { message ->
            // EXP-1459: a locked card cannot be answered here: never a card, never docked.
            if (message.locked != null) return@mapNotNull null
            val card = message.card ?: return@mapNotNull null
            val kind = (card["kind"] as? kotlinx.serialization.json.JsonPrimitive)
                ?.takeIf { it.isString }?.content
            val engine = card["questionId"].let { it != null && it !is kotlinx.serialization.json.JsonNull }
            when {
                kind == DomainContract.messageCardKindPlan && !engine -> {
                    val decision = card["decision"]
                    val waiting = decision == null || decision is kotlinx.serialization.json.JsonNull
                    message to SessionCard(message.id, SessionCardKind.Plan, waiting)
                }
                kind == DomainContract.messageCardKindQuestion && !engine ->
                    message to SessionCard(message.id, SessionCardKind.Question, message.answeredAt == null)
                kind == DomainContract.messageCardKindSurface ->
                    message to SessionCard(message.id, SessionCardKind.Surface, waiting = false)
                else -> null
            }
        }
        // The time parsed once per card ([sortedByEpoch]), ties by id.
        .sortedByEpoch(tie = compareBy { it.first.id }) { WireTimestamps.parseEpochMs(it.first.createdAt) ?: 0L }
        .map { it.second }

    /** A settled feed call as the anchoring reads it; #FEED-231: [cardId] =
     *  the card it posted or edited (its `preview.id`). */
    data class FeedCall(val id: Long, val name: String?, val settled: Boolean, val failed: Boolean, val cardId: String? = null)

    /** Where each session card draws: right after the SETTLED, unfailed call
     *  that posted it, paired newest back PER KIND; a card with no call left =
     *  [Anchors.unanchored] (card order, the transcript's head). */
    data class Anchors(val after: Map<Long, List<String>>, val unanchored: List<String>)

    /** S-3: THE call that posts every kind now (`exponential_show`). */
    const val SHOW_CARD_CALL = "show"

    /** The row a card-posting call resolves to: a contract row (`show`), else
     *  a retired legacy row by the same prefix rule, else null. */
    fun cardCallRow(name: String?): String? {
        ExpToolDisplay.row(name)?.let { return it }
        val trimmed = name?.trim().orEmpty()
        return SessionCardKind.entries.map { it.postingCall }
            .firstOrNull { trimmed.endsWith(DomainContract.expToolPrefix + it) }
    }

    /** #FEED-231: the call naming the card (the newest; a named call never
     *  joins the count pool), else legacy calls of the card's kind, then the
     *  shared `show` calls (S-3), counting only calls with no card id. */
    fun anchorSessionCards(calls: List<FeedCall>, cards: List<SessionCard>): Anchors {
        val byRow = HashMap<String, MutableList<Long>>()
        val named = HashMap<String, Long>()
        for (call in calls) {
            if (!call.settled || call.failed) continue
            val row = cardCallRow(call.name) ?: continue
            val card = call.cardId
            if (!card.isNullOrEmpty()) {
                named[card] = call.id
                continue
            }
            byRow.getOrPut(row) { mutableListOf() }.add(call.id)
        }
        val used = HashMap<String, Int>()
        fun take(row: String): Long? {
            val skip = used[row] ?: 0
            val ids = byRow[row].orEmpty()
            val call = ids.getOrNull(ids.size - 1 - skip)
            if (call != null) used[row] = skip + 1
            return call
        }
        val after = LinkedHashMap<Long, MutableList<String>>()
        val unanchored = ArrayList<String>()
        for (card in cards.asReversed()) {
            val call = named[card.id] ?: take(card.kind.postingCall) ?: take(SHOW_CARD_CALL)
            if (call == null) unanchored.add(0, card.id) else after.getOrPut(call) { mutableListOf() }.add(0, card.id)
        }
        return Anchors(after, unanchored)
    }

    /** The feed's tool calls, as [anchorSessionCards] reads them. */
    fun feedCalls(feed: List<AgentFeedItem>): List<FeedCall> = feed.mapNotNull { item ->
        (item as? AgentFeedItem.Tool)?.let { FeedCall(it.id, it.name, it.settled, it.failed, it.preview?.id) }
    }
}
