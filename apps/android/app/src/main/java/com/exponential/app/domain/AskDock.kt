package com.exponential.app.domain

/**
 * THE ask cards ×4 (fixtures/ask-dock.json; TS reference
 * `packages/domain-contract/src/ask-dock.ts`, desktop `domain::ask_dock`,
 * iOS ExpCore `AskDock.swift`): N-12: a question or plan card waiting on the
 * viewer is a DockCard of THE Dock ([Dock], fixtures/dock.json); the composer
 * never leaves. This file keeps the cards' own words: the label and title,
 * the plan options, a thread's waiting cards. `AskDockTest` replays the
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
        /** The asking session's name in a thread; null in the session view. */
        val sessionName: String?,
        /** Order, oldest first. */
        val createdAt: Long,
        /** A plan's question count (a question: null). */
        val questions: Int? = null,
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

    /** The option a decided plan card names as its answer. */
    fun planDecisionLabel(decision: String, fresh: Boolean): String {
        // FEED-140: a plan the agent replaced with a newer one.
        if (decision == DECISION_SUPERSEDED) return Copy.SUPERSEDED
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

    /** A thread's waiting cards, Conversation order: every plan row with no
     *  decision and every unanswered question row, of ANY session in the
     *  tree; [sessionName] names the asking session (null = unnamed);
     *  a plan carries its question count. */
    fun threadPendingAsks(
        rows: List<SessionTree.ThreadRow>,
        sessionName: (String) -> String?,
    ): List<Pending> = rows.mapIndexedNotNull { index, row ->
        when {
            row is SessionTree.ThreadRow.Plan && row.decision == null ->
                Pending(row.id, Kind.Plan, row.plan, sessionName(row.sessionId), index.toLong(), row.questions.size)
            row is SessionTree.ThreadRow.Question && !row.answered ->
                Pending(row.id, Kind.Question, row.question, sessionName(row.sessionId), index.toLong())
            else -> null
        }
    }

    /** A thread's card captions: each session TAB's name (nested ones
     *  included); the root (no tab) = null. */
    fun threadSessionNames(tabs: List<SessionTree.ThreadTab>): (String) -> String? {
        val names = tabs.filterIsInstance<SessionTree.ThreadTab.SessionTab>().flatMap { tab ->
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
        .sortedWith(
            compareBy<Pair<SessionTree.Message, SessionCard>>(
                { WireTimestamps.parseEpochMs(it.first.createdAt) ?: 0L },
                { it.first.id },
            ),
        )
        .map { it.second }

    /** A settled feed call as the anchoring reads it. */
    data class FeedCall(val id: Long, val name: String?, val settled: Boolean, val failed: Boolean)

    /** Where each session card draws: right after the SETTLED, unfailed call
     *  that posted it, paired newest back PER KIND; a card with no call left =
     *  [Anchors.unanchored] (card order, the transcript's head). */
    data class Anchors(val after: Map<Long, List<String>>, val unanchored: List<String>)

    fun anchorSessionCards(calls: List<FeedCall>, cards: List<SessionCard>): Anchors {
        val byRow = HashMap<String, MutableList<Long>>()
        for (call in calls) {
            if (!call.settled || call.failed) continue
            val row = ExpToolDisplay.row(call.name) ?: continue
            byRow.getOrPut(row) { mutableListOf() }.add(call.id)
        }
        val used = HashMap<String, Int>()
        val after = LinkedHashMap<Long, MutableList<String>>()
        val unanchored = ArrayList<String>()
        for (card in cards.asReversed()) {
            val row = card.kind.postingCall
            val skip = used[row] ?: 0
            used[row] = skip + 1
            val ids = byRow[row].orEmpty()
            val call = ids.getOrNull(ids.size - 1 - skip)
            if (call == null) unanchored.add(0, card.id) else after.getOrPut(call) { mutableListOf() }.add(0, card.id)
        }
        return Anchors(after, unanchored)
    }

    /** The feed's tool calls, as [anchorSessionCards] reads them. */
    fun feedCalls(feed: List<AgentFeedItem>): List<FeedCall> = feed.mapNotNull { item ->
        (item as? AgentFeedItem.Tool)?.let { FeedCall(it.id, it.name, it.settled, it.failed) }
    }
}
