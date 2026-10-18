package com.exponential.app.domain

/**
 * THE ask dock ×4 (fixtures/ask-dock.json; TS reference
 * `packages/domain-contract/src/ask-dock.ts`, desktop `domain::ask_dock`,
 * iOS ExpCore `AskDock.swift`): the question/plan cards waiting on the
 * viewer take the composer's slot, in the session view AND the thread. One
 * pending card = the big card; several = compact cards stacked, a tap
 * focuses one (the back arrow returns to the stack). `AskDockTest` replays
 * the fixture and compares whole objects.
 */
object AskDock {

    object Copy {
        const val PLAN_READY = "Plan ready"
        const val QUESTION = "Question"
        const val BACK = "Back to all cards"
        const val FEEDBACK_PLACEHOLDER = "Tell the agent what to change…"
    }

    /** Icon CONCEPT names (icons.json). */
    object Icons {
        const val PLAN = "coding-plan"
        const val QUESTION = "ui-help"
        const val BACK = "ui-back"
        const val OPEN = "ui-chevron-right"
    }

    object TestIds {
        const val DOCK = "ask-dock"
        const val ROW = "ask-dock-row"
        const val BACK = "ask-dock-back"
        const val CARD = "ask-dock-card"
    }

    /** The big card's body cap, a fraction of the viewport height. */
    const val BODY_MAX_HEIGHT_FRACTION = 0.4f

    /** The stack's gap (dp). */
    const val STACK_GAP = 6

    enum class Kind(val wire: String) { Plan("plan"), Question("question") }

    data class Pending(
        /** The card's id (a `messages.id`, or the session view's ask id). */
        val id: String,
        val kind: Kind,
        /** The plan, or the question's text (markdown). */
        val text: String,
        /** The asking session's name in a thread; null in the session view. */
        val sessionName: String?,
        /** Stack order, oldest first. */
        val createdAt: Long,
    )

    data class Row(
        val id: String,
        val kind: Kind,
        val label: String,
        val title: String,
        val caption: String?,
    )

    enum class Mode(val wire: String) { None("none"), Single("single"), Stack("stack"), Focused("focused") }

    data class View(
        val mode: Mode,
        /** Whether the composer yields its slot to the dock. */
        val composerHidden: Boolean,
        /** The back arrow (only a focused card out of a stack). */
        val back: Boolean,
        /** The big card's id (single | focused), else null. */
        val cardId: String?,
        /** The compact cards (stack only), oldest first. */
        val rows: List<Row>,
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

    fun askDock(pending: List<Pending>, focusedId: String?): View {
        val sorted = pending.sortedWith(compareBy<Pending>({ it.createdAt }, { it.id }))
        if (sorted.isEmpty()) return View(Mode.None, composerHidden = false, back = false, cardId = null, rows = emptyList())
        if (sorted.size == 1) return View(Mode.Single, composerHidden = true, back = false, cardId = sorted[0].id, rows = emptyList())
        val focused = sorted.firstOrNull { it.id == focusedId }
        if (focused != null) return View(Mode.Focused, composerHidden = true, back = true, cardId = focused.id, rows = emptyList())
        return View(
            Mode.Stack,
            composerHidden = true,
            back = false,
            cardId = null,
            rows = sorted.map { p ->
                val title = askDockTitle(p.text)
                Row(p.id, p.kind, label(p.kind), title.ifEmpty { label(p.kind) }, p.sessionName)
            },
        )
    }

    /** A thread's waiting cards, Conversation order: every plan row with no
     *  decision and every unanswered question row, of ANY session in the
     *  tree; [sessionName] names the asking session (null = unnamed). */
    fun threadPendingAsks(
        rows: List<SessionTree.ThreadRow>,
        sessionName: (String) -> String?,
    ): List<Pending> = rows.mapIndexedNotNull { index, row ->
        when {
            row is SessionTree.ThreadRow.Plan && row.decision == null ->
                Pending(row.id, Kind.Plan, row.plan, sessionName(row.sessionId), index.toLong())
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
