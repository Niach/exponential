package com.exponential.app.domain

/**
 * N-12: THE Dock ×4 (fixtures/dock.json; TS reference
 * `packages/domain-contract/src/dock.ts`, desktop `domain::dock`, iOS ExpCore
 * `Dock.swift`): the floating band over the transcript's bottom edge holding
 * the DockCards (a waiting plan or question, THE Queue, the queued messages,
 * a status banner) above THE composer, which never leaves. Several cards
 * pile like the toasts ([ToastStack.stackLayout] with [STACK]); a tap opens
 * a card's bottom sheet. `DockTest` replays the fixture and compares whole
 * objects. The Compose half lives in `ui/components/Dock.kt`.
 */
object Dock {

    object Copy {
        const val QUEUED_ONE = "1 queued"
        const val QUEUED_MANY = "{count} queued"
        const val QUESTIONS_ONE = "1 question"
        const val QUESTIONS_MANY = "{count} questions"
        const val CLOSE = "Close"
        const val SHOW_ALL = "Show all"
    }

    /** Icon CONCEPT names (icons.json). */
    object Icons {
        const val PLAN = "coding-plan"
        const val QUESTION = "ui-help"
        const val TASKS = "trace-todo"
        const val QUEUED = "ui-queued"
        const val OPEN = "ui-chevron-right"
        const val CLOSE = "ui-close"
        const val SUGGESTIONS = "ui-checklist"
    }

    object TestIds {
        const val DOCK = "dock"
        const val PILE = "dock-pile"
        const val CARD = "dock-card"
        const val SHEET = "dock-sheet"
        const val SHEET_BODY = "dock-sheet-body"
        const val SHEET_FOOTER = "dock-sheet-footer"
        const val CLOSE = "dock-sheet-close"
    }

    /** The pile's numbers (`stack`): the toast's peek/scale/visible, its own gap. */
    val STACK = StackConstants(peek = 14.0, scaleStep = 0.05, visible = 3, gap = 6.0)

    /** The sheet (`sheet`): at most this fraction of the viewport, this top radius (dp). */
    const val SHEET_MAX_HEIGHT_FRACTION = 0.85f
    const val SHEET_RADIUS = 24

    enum class Kind(val wire: String) {
        Plan("plan"), Question("question"), Tasks("tasks"), Queued("queued"), Suggestions("suggestions"), Banner("banner");

        val isAsk: Boolean get() = this == Plan || this == Question
    }

    enum class Tint(val wire: String) { Plan("plan"), Question("question"), Muted("muted"), Warning("warning") }

    /** The `order`: queued, tasks, suggestions, banner, then the asks. */
    val ORDER: List<String> = listOf("queued", "tasks", "suggestions", "banner", "ask")

    sealed interface Source {
        data class Ask(
            val id: String,
            val kind: AskDock.Kind,
            /** The plan, or the question (markdown). */
            val text: String,
            /** The asking session's name in a thread; null in the session view. */
            val sessionName: String?,
            /** How many questions a plan asks (a question: 0). */
            val questions: Int,
            /** The asks' order, oldest first. */
            val createdAt: Long,
        ) : Source

        /** THE Queue (task-queue.json `taskQueue`), or null when nothing is to do. */
        data class Tasks(val queue: TaskQueue?) : Source

        data class Queued(val messages: List<QueuedMessage>) : Source

        /** EXP-1323's suggested-tasks row while any item is open. */
        data class Suggestions(
            /** The suggestions row's id (a `messages.id`). */
            val id: String,
            /** The header's count (`3 suggested tasks`). */
            val label: String,
            /** The header's `started` part, else null. */
            val caption: String?,
            /** The first open item's title. */
            val title: String,
            /** How many items are still open. */
            val open: Int,
        ) : Source

        data class Banner(
            val id: String,
            /** Concept icon. */
            val icon: String,
            val label: String,
            val text: String,
            /** `muted` | `warning`. */
            val tone: Tint,
            /** The banner's one button word, else null. */
            val action: String?,
        ) : Source
    }

    data class QueuedMessage(val id: String, val text: String)

    data class Row(
        val id: String,
        val kind: Kind,
        val tint: Tint,
        /** Concept icon. */
        val icon: String,
        val label: String,
        val caption: String?,
        val meta: String?,
        val title: String,
        /** A tap opens the sheet. */
        val expandable: Boolean,
        /** A banner's button word, else null. */
        val action: String?,
    )

    enum class Mode(val wire: String) { None("none"), Single("single"), Pile("pile"), Fanned("fanned") }

    data class View(
        val mode: Mode,
        /** Top to bottom; the front card is the LAST. */
        val rows: List<Row>,
        val frontId: String?,
        /** The open sheet's card, when that card exists and is expandable. */
        val openId: String?,
        /** How many piled cards stay out of sight (beyond `stack.visible`). */
        val hidden: Int,
    )

    private fun counted(n: Int, one: String, many: String): String =
        if (n == 1) one else many.replace("{count}", n.toString())

    private fun order(kind: Kind): Int = ORDER.indexOf(if (kind.isAsk) "ask" else kind.wire)

    /** The cards in the Dock's order (queued, tasks, suggestions, banners, then the asks
     *  oldest first, ties by id). An empty queue or no queued message draws nothing. */
    fun dockRows(sources: List<Source>): List<Row> {
        val rows = mutableListOf<Pair<Row, Long>>()
        for (source in sources) {
            when (source) {
                is Source.Queued -> {
                    val first = source.messages.firstOrNull() ?: continue
                    rows += Row(
                        id = "queued",
                        kind = Kind.Queued,
                        tint = Tint.Muted,
                        icon = Icons.QUEUED,
                        label = counted(source.messages.size, Copy.QUEUED_ONE, Copy.QUEUED_MANY),
                        caption = null,
                        meta = null,
                        title = first.text,
                        expandable = true,
                        action = null,
                    ) to 0L
                }
                is Source.Tasks -> {
                    val queue = source.queue ?: continue
                    val first = queue.todo.items.firstOrNull() ?: continue
                    rows += Row(
                        id = "tasks",
                        kind = Kind.Tasks,
                        tint = Tint.Muted,
                        icon = Icons.TASKS,
                        label = queue.todo.title,
                        caption = queue.completed?.title,
                        meta = first.description,
                        title = first.label,
                        expandable = true,
                        action = null,
                    ) to 0L
                }
                is Source.Suggestions -> rows += Row(
                    id = source.id,
                    kind = Kind.Suggestions,
                    tint = Tint.Muted,
                    icon = Icons.SUGGESTIONS,
                    label = source.label,
                    caption = source.caption,
                    meta = null,
                    title = source.title,
                    expandable = true,
                    action = null,
                ) to 0L
                is Source.Banner -> rows += Row(
                    id = "banner:${source.id}",
                    kind = Kind.Banner,
                    tint = source.tone,
                    icon = source.icon,
                    label = source.label,
                    caption = null,
                    meta = null,
                    title = source.text,
                    expandable = false,
                    action = source.action,
                ) to 0L
                is Source.Ask -> {
                    val plan = source.kind == AskDock.Kind.Plan
                    val label = AskDock.label(source.kind)
                    val title = AskDock.askDockTitle(source.text)
                    rows += Row(
                        id = source.id,
                        kind = if (plan) Kind.Plan else Kind.Question,
                        tint = if (plan) Tint.Plan else Tint.Question,
                        icon = if (plan) Icons.PLAN else Icons.QUESTION,
                        label = label,
                        caption = source.sessionName,
                        meta = if (plan && source.questions > 0) {
                            counted(source.questions, Copy.QUESTIONS_ONE, Copy.QUESTIONS_MANY)
                        } else {
                            null
                        },
                        title = title.ifEmpty { label },
                        expandable = true,
                        action = null,
                    ) to source.createdAt
                }
            }
        }
        return rows
            .sortedWith(compareBy<Pair<Row, Long>>({ order(it.first.kind) }, { it.second }, { it.first.id }))
            .map { it.first }
    }

    /** The band: the pile (or the fan while [expanded]), the front card, the open sheet. */
    fun dock(rows: List<Row>, expanded: Boolean, openId: String?): View {
        val open = openId?.let { id -> rows.firstOrNull { it.id == id } }
        val mode = when {
            rows.isEmpty() -> Mode.None
            rows.size == 1 -> Mode.Single
            expanded -> Mode.Fanned
            else -> Mode.Pile
        }
        return View(
            mode = mode,
            rows = rows,
            frontId = rows.lastOrNull()?.id,
            openId = open?.takeIf { it.expandable }?.id,
            hidden = if (mode == Mode.Pile) maxOf(0, rows.size - STACK.visible) else 0,
        )
    }

    /** The ask (plan or question) that just arrived, front-most first, else
     *  null. [previous] = every card id seen before this frame. */
    fun dockAutoOpen(previous: Collection<String>, rows: List<Pair<String, Kind>>): String? {
        val seen = previous.toSet()
        return rows.lastOrNull { (id, kind) -> kind.isAsk && id !in seen }?.first
    }

    /** One suggestions row as [suggestionSources] reads it. */
    data class SuggestionRow(val id: String, val header: String, val items: List<Pair<String, String>>)

    /** EXP-1323's suggested-tasks rows with any OPEN item, as Dock sources:
     *  the header splits at ` · ` into the label (the count) and the started
     *  caption; the title is the first open item's. A row with no open item
     *  is none of the Dock's (it stays in the Conversation). [rows] items are
     *  (state, title) pairs. */
    fun suggestionSources(rows: List<SuggestionRow>): List<Source.Suggestions> = rows.mapNotNull { row ->
        val open = row.items.filter { it.first == "open" }
        if (open.isEmpty()) return@mapNotNull null
        val parts = row.header.split(" · ")
        Source.Suggestions(
            id = row.id,
            label = parts[0],
            caption = parts.getOrNull(1),
            title = open.first().second,
            open = open.size,
        )
    }
}
