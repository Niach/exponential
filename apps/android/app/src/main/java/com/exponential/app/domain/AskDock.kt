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

    /** The option a decided plan card names as its answer. */
    fun planDecisionLabel(decision: String, fresh: Boolean): String {
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
}
