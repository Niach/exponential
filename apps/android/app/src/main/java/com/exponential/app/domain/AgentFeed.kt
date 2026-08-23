package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray

// The activity feed's model and its pure reducer — the "Agent session" chat
// view's whole vocabulary (EXP-32/EXP-249), compose-free and unit testable on
// its own. It lives in `domain` because BOTH sides need it: the socket in
// `data/steer` folds events into it, the screen in `ui/session` renders the
// result — mirroring iOS's ExpCore `AgentFeed.swift` and web's
// `lib/agent-feed.ts`.

/** Client-side feed cap — matches the relay's ACTIVITY_LOG_CAP (EXP-249). */
const val FEED_CAP = 2000

/** One answer choice of a [AgentFeedItem.Question] — `key` is the raw
 *  keystroke that selects it in the desktop TUI picker (mapped desktop-side)
 *  and the value echoed back in a semantic `answer` frame. */
data class QuestionOption(
    val label: String,
    val key: String,
    val description: String? = null,
    /** EXP-513: claude's synthetic free-text row ("Type something.") —
     *  selecting it reveals an inline input and the typed reply rides the
     *  answer frame's `text`. Absent from older desktops. */
    val freeText: Boolean = false,
)

/** This client's send state for one question card (EXP-249): the card locks
 *  the instant an answer goes out and stays locked through the desktop's
 *  `answer_ack`; a missing ack flips it to [AnswerState.Failed] after
 *  [ANSWER_ACK_TIMEOUT_MS] — answerable again, with a visible retry hint
 *  instead of a silent rollback (EXP-334, web parity). */
enum class AnswerState { Sending, Acked, Failed }

/** Whether a lock in [state] still holds the card (a Failed one doesn't). */
fun AnswerState?.locksCard(): Boolean = this == AnswerState.Sending || this == AnswerState.Acked

/** One rendered feed entry. Diffs never enter the feed — see [ActivityFeedState.latestDiff]. */
sealed interface AgentFeedItem {
    val id: Long

    data class Narration(override val id: Long, val text: String) : AgentFeedItem

    data class Tool(
        override val id: Long,
        val name: String,
        val detail: String?,
        /** Set when the call came from a subagent (EXP-249) — the row renders
         *  inside that subagent's group, not in the main feed. */
        val subagentId: String? = null,
    ) : AgentFeedItem

    /** A human turn (EXP-78): the initial prompt or a steered message. */
    data class UserMessage(override val id: Long, val text: String) : AgentFeedItem

    /** An interactive question (AskUserQuestion / plan approval, EXP-78).
     *  [planMode] marks an ExitPlanMode plan-approval picker (EXP-97) —
     *  presentation-only, absent on events from older desktops/relays.
     *  [resolved]/[answer] come from the desktop's `question_resolved` event
     *  (EXP-249), or from the legacy `Question answered:` / `Question
     *  dismissed.` narrations when the card carries no [wireId] — a resolved
     *  card renders its answer and is never answerable again. */
    data class Question(
        override val id: Long,
        val text: String,
        val options: List<QuestionOption>,
        val multiSelect: Boolean,
        val planMode: Boolean = false,
        val resolved: Boolean = false,
        val answer: String? = null,
        /** Stable wire identity (EXP-249). Present ⇒ answerable with a
         *  semantic `answer` frame and re-emissions replace the card in place;
         *  absent ⇒ a pre-EXP-249 card driven by raw keystrokes. */
        val wireId: String? = null,
        /** Groups the steps of one multi-question ask into a stepper card. */
        val askId: String? = null,
        /** 1-based step position; absent on the ask's final submit step. */
        val index: Int? = null,
        val total: Int? = null,
        val header: String? = null,
    ) : AgentFeedItem

    /** A subagent's lifecycle row (EXP-249) — the header of the collapsible
     *  group its tool calls render inside. */
    data class Subagent(
        override val id: Long,
        val subagentId: String,
        val agentType: String,
        val completed: Boolean,
        val detail: String? = null,
    ) : AgentFeedItem

    /** A permission prompt the agent hit (EXP-249) — informational only: it is
     *  answered on the desktop, never from here. */
    data class Permission(
        override val id: Long,
        val tool: String,
        val detail: String? = null,
    ) : AgentFeedItem
}

/** The key a question card's [AnswerState] is tracked under: its wire id, or a
 *  local stand-in for pre-EXP-249 cards that have none. */
fun questionLockKey(item: AgentFeedItem.Question): String =
    item.wireId ?: "local:${item.id}"

/** The desktop's plan-picker resolution narration (steer/src/activity.rs) —
 *  the no-protocol-change signal that a pending plan approval was answered. */
const val PLAN_RESOLVED_NARRATION = "Plan approval answered."

/** The desktop's answered-question narration prefix (steer/src/activity.rs,
 *  EXP-197): one `Question answered: <answer>` narration per question flushes
 *  with the transcript once an AskUserQuestion resolves — folded into the
 *  earliest unanswered question card instead of rendering as a narration. */
const val QUESTION_ANSWERED_PREFIX = "Question answered: "

/** The desktop's dismissed-question narration (EXP-197) — the ask resolved
 *  WITHOUT answers (Esc / rejected); retires every pending question card. */
const val QUESTION_DISMISSED_NARRATION = "Question dismissed."

/** `subagent.agentType` when the desktop's hook payload carried none — old
 *  desktop builds also stamp it onto the COMPLETED edge, so it is a sentinel
 *  the label selection skips past, never a type to prefer (EXP-350). */
const val SUBAGENT_FALLBACK_TYPE = "agent"

/** Fold an answer into the EARLIEST unanswered non-plan question card
 *  (answers arrive in question order, so earliest-first keeps multi-question
 *  asks aligned). Legacy path only — cards carrying a wire id resolve through
 *  [resolveQuestions]. Null when no card is waiting: the caller then falls
 *  back to rendering the narration so the answer is never lost. */
fun attachQuestionAnswer(feed: List<AgentFeedItem>, answer: String): List<AgentFeedItem>? {
    val index = feed.indexOfFirst {
        it is AgentFeedItem.Question && it.wireId == null && !it.planMode && !it.resolved
    }
    if (index < 0) return null
    val item = feed[index] as AgentFeedItem.Question
    return feed.toMutableList().apply {
        this[index] = item.copy(resolved = true, answer = answer)
    }
}

/** Retire every pending non-plan legacy question card (the ask was dismissed).
 *  Null when nothing was pending. */
fun dismissPendingQuestions(feed: List<AgentFeedItem>): List<AgentFeedItem>? {
    fun pending(it: AgentFeedItem) =
        it is AgentFeedItem.Question && it.wireId == null && !it.planMode && !it.resolved
    if (feed.none(::pending)) return null
    return feed.map { if (pending(it)) (it as AgentFeedItem.Question).copy(resolved = true) else it }
}

/** Append a question card, or REPLACE the card carrying the same wire id in
 *  place (EXP-249) — a re-emission augments an ask (options the desktop
 *  discovers later) and must never stack a second card. The local feed id and
 *  any resolution already folded in are preserved. */
fun upsertQuestion(feed: List<AgentFeedItem>, item: AgentFeedItem.Question): List<AgentFeedItem> {
    val wireId = item.wireId ?: return feed + item
    val index = feed.indexOfFirst { it is AgentFeedItem.Question && it.wireId == wireId }
    if (index < 0) return feed + item
    val existing = feed[index] as AgentFeedItem.Question
    return feed.toMutableList().apply {
        this[index] = item.copy(
            id = existing.id,
            resolved = existing.resolved || item.resolved,
            answer = item.answer ?: existing.answer,
        )
    }
}

/** Insert [item] immediately BEFORE the first question card matching [anchor]
 *  (its ask id or wire id) — EXP-483: claude withholds the transcript entry
 *  carrying an ask/plan tool_use, prose included, until the picker resolves,
 *  so that prose arrives AFTER the already-published card and tags itself
 *  with `beforeQuestionId` to be spliced back above it. Matches resolved
 *  cards too (the twin normally flushes post-answer). Null when no card
 *  matches (evicted, legacy producer) — the caller appends. */
fun spliceBeforeQuestion(
    feed: List<AgentFeedItem>,
    anchor: String,
    item: AgentFeedItem,
): List<AgentFeedItem>? {
    val index = feed.indexOfFirst {
        it is AgentFeedItem.Question && (it.askId == anchor || it.wireId == anchor)
    }
    if (index < 0) return null
    return feed.toMutableList().apply { add(index, item) }
}

/** Fold a `question_resolved` event into the feed (EXP-249): retire the card
 *  named by [id], else every card of [askId] — whose [answers] map onto the
 *  ask's steps in step order. Null when nothing matched, so the caller can
 *  fall back to the legacy narration path. */
fun resolveQuestions(
    feed: List<AgentFeedItem>,
    id: String?,
    askId: String?,
    answers: List<String> = emptyList(),
    dismissed: Boolean = false,
): List<AgentFeedItem>? {
    if (id != null) {
        val index = feed.indexOfFirst { it is AgentFeedItem.Question && it.wireId == id }
        if (index < 0) return null
        val item = feed[index] as AgentFeedItem.Question
        return feed.toMutableList().apply {
            this[index] = item.copy(
                resolved = true,
                answer = if (dismissed) item.answer else answers.firstOrNull() ?: item.answer,
            )
        }
    }
    if (askId == null) return null
    val steps = orderedSteps(feed.filterIsInstance<AgentFeedItem.Question>().filter { it.askId == askId })
    if (steps.isEmpty()) return null
    val answerByFeedId = steps.withIndex().associate { (i, step) -> step.id to answers.getOrNull(i) }
    return feed.map { item ->
        if (item is AgentFeedItem.Question && item.askId == askId) {
            item.copy(
                resolved = true,
                answer = if (dismissed) item.answer else answerByFeedId[item.id] ?: item.answer,
            )
        } else {
            item
        }
    }
}

/** Flip a running subagent row to completed in place (EXP-249) — a second row
 *  for the same subagent would split its tool group. Null when none is
 *  running, so the caller appends instead. */
fun completeSubagent(
    feed: List<AgentFeedItem>,
    subagentId: String,
    detail: String?,
): List<AgentFeedItem>? {
    val index = feed.indexOfLast {
        it is AgentFeedItem.Subagent && it.subagentId == subagentId && !it.completed
    }
    if (index < 0) return null
    val item = feed[index] as AgentFeedItem.Subagent
    return feed.toMutableList().apply {
        this[index] = item.copy(completed = true, detail = detail ?: item.detail)
    }
}

/**
 * Ids of the [AgentFeedItem.Question] items still answerable.
 *
 * A card carrying a wire id (EXP-249) is answerable until its own
 * `question_resolved` lands — the desktop's structured stream states question
 * lifetime outright, so no position guessing applies to it.
 *
 * Cards without one keep the EXP-174 heuristic: the TRAILING consecutive
 * question run (any later event means the desktop TUI moved on), PLUS any
 * plan-approval question with no resolution signal after it. Plan questions
 * are published from the live terminal grid the moment the picker appears,
 * while the transcript tail lags — so tool rows and narration can flush in
 * BEHIND a plan card whose picker is still on screen. Only a newer question or
 * the desktop's explicit [PLAN_RESOLVED_NARRATION] proves a plan picker
 * actually resolved — a human message does NOT (steering mid-plan leaves the
 * picker up).
 */
fun activeQuestionIds(feed: List<AgentFeedItem>): Set<Long> {
    val ids = mutableSetOf<Long>()
    for (item in feed) {
        if (item is AgentFeedItem.Question && item.wireId != null && !item.resolved) ids.add(item.id)
    }
    // Still inside the trailing consecutive question run.
    var trailing = true
    // A resolution signal lies after the current position.
    var retired = false
    for (item in feed.asReversed()) {
        when (item) {
            is AgentFeedItem.Question -> {
                if (item.resolved) {
                    // An answered/dismissed card is itself a resolution signal
                    // (it proves the TUI moved past it) and is never active.
                    trailing = false
                    retired = true
                } else {
                    if (item.wireId == null && (trailing || (item.planMode && !retired))) {
                        ids.add(item.id)
                    }
                    retired = true
                }
            }
            // A human message breaks the trailing run but does NOT retire a
            // plan card — steering a message mid-plan leaves the picker up
            // (web parity, EXP-249).
            is AgentFeedItem.UserMessage -> trailing = false
            is AgentFeedItem.Narration -> {
                trailing = false
                if (item.text.trim() == PLAN_RESOLVED_NARRATION) retired = true
            }
            else -> trailing = false
        }
        if (retired && !trailing) break
    }
    return ids
}

/** One render row over the flat feed: a single item, a run of ≥2 CONSECUTIVE
 *  tool calls collapsed into one "N tool calls" row (EXP-97), one subagent
 *  with the calls it made, or every step of one multi-question ask (EXP-249).
 *  A row's id is the LOWEST feed id it covers, so the row key (and its
 *  expanded state) stays stable while the row keeps growing. */
sealed interface AgentFeedRow {
    val id: Long

    data class Single(val item: AgentFeedItem) : AgentFeedRow {
        override val id get() = item.id
    }

    data class ToolRun(val items: List<AgentFeedItem.Tool>) : AgentFeedRow {
        override val id get() = items.first().id
    }

    /** Every step of one askId ask, in step order — the card shows ONE step at
     *  a time and advances as the answers are acknowledged. */
    data class QuestionStepper(
        val askId: String,
        val steps: List<AgentFeedItem.Question>,
    ) : AgentFeedRow {
        // Not steps.first(): a late-arriving low-index step re-sorts to the
        // head, and the row key must not move with it.
        override val id get() = steps.minOf { it.id }
    }

    /** A subagent's run, derived from every marker and tagged tool call in the
     *  feed (EXP-350 — grouping is by id across the WHOLE feed, iOS/web
     *  parity, so an interleaved fan-out never splits a group). The label is
     *  the first marker's REAL type: a later marker carrying the fallback (an
     *  old desktop's completed edge) can never degrade it. */
    data class SubagentRun(
        override val id: Long,
        val subagentId: String,
        val agentType: String,
        val completed: Boolean,
        val detail: String?,
        val tools: List<AgentFeedItem.Tool>,
    ) : AgentFeedRow
}

/** Render-time projection of the flat feed — a pure function: the feed itself
 *  (and [activeQuestionIds] over it) is never restructured.
 *  - a subagent's markers and its tagged tool calls collapse into ONE row by
 *    id across the whole feed (EXP-350 — an interleaved fan-out used to strand
 *    every group's tools), anchored where the group's first item landed,
 *  - all question cards sharing an askId collapse into ONE stepper row,
 *    anchored where the ask's first card landed,
 *  - runs of ≥2 consecutive PLAIN tool calls collapse into one "N tool calls"
 *    row (a tagged tool belongs to its subagent, never to a main-thread run). */
fun groupFeedRows(feed: List<AgentFeedItem>): List<AgentFeedRow> {
    val stepsByAsk = feed.filterIsInstance<AgentFeedItem.Question>()
        .filter { it.askId != null }
        .groupBy { it.askId!! }
    val markersBySubagent = feed.filterIsInstance<AgentFeedItem.Subagent>()
        .groupBy { it.subagentId }
    val toolsBySubagent = feed.filterIsInstance<AgentFeedItem.Tool>()
        .filter { it.subagentId != null }
        .groupBy { it.subagentId!! }
    val emittedAsks = mutableSetOf<String>()
    val emittedSubagents = mutableSetOf<String>()
    val rows = mutableListOf<AgentFeedRow>()
    var i = 0
    while (i < feed.size) {
        val item = feed[i]
        val subagentId = when {
            item is AgentFeedItem.Subagent -> item.subagentId
            item is AgentFeedItem.Tool -> item.subagentId
            else -> null
        }
        when {
            subagentId != null -> {
                if (emittedSubagents.add(subagentId)) {
                    val markers = markersBySubagent[subagentId].orEmpty()
                    val tools = toolsBySubagent[subagentId].orEmpty()
                    val types = markers.map { it.agentType }.filter { it.isNotBlank() }
                    rows.add(
                        AgentFeedRow.SubagentRun(
                            id = (markers.map { it.id } + tools.map { it.id }).min(),
                            subagentId = subagentId,
                            agentType = types.firstOrNull { it != SUBAGENT_FALLBACK_TYPE }
                                ?: types.firstOrNull()
                                ?: SUBAGENT_FALLBACK_TYPE,
                            completed = markers.any { it.completed },
                            detail = markers.lastOrNull { it.detail != null }?.detail,
                            tools = tools,
                        ),
                    )
                }
                i++
            }
            item is AgentFeedItem.Question && item.askId != null -> {
                if (emittedAsks.add(item.askId)) {
                    rows.add(
                        AgentFeedRow.QuestionStepper(
                            item.askId,
                            orderedSteps(stepsByAsk[item.askId].orEmpty()),
                        ),
                    )
                }
                i++
            }
            item is AgentFeedItem.Tool -> {
                var end = i
                while (end + 1 < feed.size) {
                    val next = feed[end + 1]
                    if (next is AgentFeedItem.Tool && next.subagentId == null) end++ else break
                }
                if (end == i) {
                    rows.add(AgentFeedRow.Single(item))
                } else {
                    rows.add(AgentFeedRow.ToolRun(feed.subList(i, end + 1).map { it as AgentFeedItem.Tool }))
                }
                i = end + 1
            }
            else -> {
                rows.add(AgentFeedRow.Single(item))
                i++
            }
        }
    }
    return rows
}

/** Ask steps in stepper order: by 1-based index, with the index-less final
 *  submit step last. */
fun orderedSteps(steps: List<AgentFeedItem.Question>): List<AgentFeedItem.Question> =
    steps.sortedWith(compareBy(nullsLast<Int>()) { it.index })

/** Every subagent seen in the feed, in first-appearance order (EXP-356) — the
 *  session screen renders one conversation tab per run, labeled and summarized
 *  exactly like its group row (iOS/web parity). */
fun collectSubagents(feed: List<AgentFeedItem>): List<AgentFeedRow.SubagentRun> =
    groupFeedRows(feed).filterIsInstance<AgentFeedRow.SubagentRun>()

/** The tabs the strip actually shows (EXP-387): running subagents, plus the
 *  focused one even when done — a completion never yanks the user out of a
 *  conversation they are reading; the tab disappears once they click away.
 *  Completed runs stay readable via their inline group row in Main. */
fun visibleSubagentTabs(
    agents: List<AgentFeedRow.SubagentRun>,
    selected: String?,
): List<AgentFeedRow.SubagentRun> =
    agents.filter { !it.completed || it.subagentId == selected }

/** The step a stepper card should show: the first one still waiting on this
 *  client, or null once every step is answered — the card then renders the
 *  whole ask with its answers. [answered] holds the lock keys of steps whose
 *  answer is out (sent or acknowledged — a dropped Sending lock re-surfaces
 *  its step). */
fun currentStepperStep(
    steps: List<AgentFeedItem.Question>,
    answered: Set<String>,
): AgentFeedItem.Question? =
    steps.firstOrNull { !it.resolved && questionLockKey(it) !in answered }

// Wire-field readers: a field of an unexpected shape reads as absent, never
// throws — one malformed event must not tear down the socket.
private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull

private fun JsonObject.bool(key: String): Boolean =
    (this[key] as? JsonPrimitive)?.booleanOrNull == true

private fun JsonObject.int(key: String): Int? = (this[key] as? JsonPrimitive)?.intOrNull

/** Everything the room's activity log owns, as ONE immutable value — the whole
 *  decode path is a pure transition over it, so it is unit testable end to end
 *  and the ViewModel is left publishing the result. */
data class ActivityFeedState(
    val feed: List<AgentFeedItem> = emptyList(),
    /** The most recent worktree diff — each one replaces the previous. */
    val latestDiff: String? = null,
    /** Per-card answer locks, keyed by [questionLockKey] (EXP-249). */
    val answerLocks: Map<String, AnswerState> = emptyMap(),
    /** What THIS client picked per locked card — the option labels (a typed
     *  free-text reply in place of its row's label). The desktop only fills a
     *  question's `answer` on `question_resolved`, which for a multi-question
     *  ask lands after the WHOLE ask submits, so answered stepper steps would
     *  otherwise read "Answered" ×N until then (EXP-588, web/iOS parity).
     *  Dropped with a failed lock — a rolled-back step has no answer. */
    val answerLabels: Map<String, List<String>> = emptyMap(),
    /** True once the desktop published a question carrying a wire id: from
     *  then on its legacy resolution narrations are duplicates of the semantic
     *  events (it emits both so pre-EXP-249 clients keep working) and must be
     *  swallowed instead of rendered. */
    val semanticQuestions: Boolean = false,
    val nextEventId: Long = 0L,
)

/**
 * Apply one `{t:'activity'}` event. Unknown kinds — and fields of an
 * unexpected shape inside a known kind — are SKIPPED, never fatal: the
 * protocol only ever grows, and one odd event must not tear the socket down.
 * [isEcho] drops a `user_message` this client already rendered locally
 * (EXP-78) and consumes the matching echo entry.
 */
fun ActivityFeedState.applyActivityEvent(
    event: JsonObject,
    isEcho: (String) -> Boolean = { false },
): ActivityFeedState = when (event.str("kind")) {
    "narration" -> {
        val text = event.str("text").orEmpty()
        val trimmed = text.trim()
        when {
            text.isBlank() -> this
            // Question-resolution signals fold into the pending card instead
            // of rendering as narration rows (EXP-197). A desktop publishing
            // structured questions emits them ONLY for pre-EXP-249 clients —
            // swallow them, `question_resolved` already did the work.
            trimmed.startsWith(QUESTION_ANSWERED_PREFIX) -> when {
                semanticQuestions -> this
                else -> {
                    val answer = trimmed.removePrefix(QUESTION_ANSWERED_PREFIX)
                    // No card waiting — render it, so the answer is never lost.
                    attachQuestionAnswer(feed, answer)?.let { withFeed(it) }
                        ?: append(AgentFeedItem.Narration(nextEventId, text))
                }
            }
            trimmed == QUESTION_DISMISSED_NARRATION ->
                if (semanticQuestions) this
                else dismissPendingQuestions(feed)?.let { withFeed(it) } ?: this
            trimmed == PLAN_RESOLVED_NARRATION && semanticQuestions -> this
            else -> {
                // EXP-483: prose from the withheld ask/plan entry flushes
                // AFTER its already-published card — splice it back above.
                val anchor = event.str("beforeQuestionId")?.takeIf { it.isNotBlank() }
                val spliced = anchor?.let {
                    spliceBeforeQuestion(feed, it, AgentFeedItem.Narration(nextEventId, text))
                }
                if (spliced != null) {
                    copy(feed = capFeed(spliced), nextEventId = nextEventId + 1)
                } else {
                    append(AgentFeedItem.Narration(nextEventId, text))
                }
            }
        }
    }
    "tool" -> {
        val name = event.str("name")
        if (name == null) {
            this
        } else {
            append(
                AgentFeedItem.Tool(
                    id = nextEventId,
                    name = name,
                    detail = event.str("detail")?.takeIf { it.isNotBlank() },
                    subagentId = event.str("subagentId")?.takeIf { it.isNotBlank() },
                ),
            )
        }
    }
    // Diffs never enter the feed — the latest replaces the previous one behind
    // the pinned "Latest changes" chip.
    "diff" -> copy(latestDiff = event.str("diff")?.takeIf { it.isNotBlank() })
    "user_message" -> {
        val text = event.str("text")
        if (text.isNullOrBlank() || isEcho(text)) this
        else append(AgentFeedItem.UserMessage(nextEventId, text))
    }
    "question" -> {
        val text = event.str("text")
        val options = runCatching {
            event["options"]!!.jsonArray.mapNotNull { raw ->
                val option = raw as? JsonObject ?: return@mapNotNull null
                val label = option.str("label") ?: return@mapNotNull null
                val key = option.str("key") ?: return@mapNotNull null
                QuestionOption(
                    label,
                    key,
                    option.str("description")?.takeIf { it.isNotBlank() },
                    freeText = option.bool("freeText"),
                )
            }
        }.getOrDefault(emptyList())
        if (text.isNullOrBlank() || options.isEmpty()) {
            this
        } else {
            val wireId = event.str("id")?.takeIf { it.isNotBlank() }
            val question = AgentFeedItem.Question(
                id = nextEventId,
                text = text,
                options = options,
                multiSelect = event.bool("multiSelect"),
                planMode = event.bool("planMode"),
                wireId = wireId,
                askId = event.str("askId")?.takeIf { it.isNotBlank() },
                index = event.int("index"),
                total = event.int("total"),
                header = event.str("header")?.takeIf { it.isNotBlank() },
            )
            val next = upsertQuestion(feed, question)
            copy(
                feed = capFeed(next),
                semanticQuestions = semanticQuestions || wireId != null,
                // A replaced card consumed no id.
                nextEventId = if (next.size > feed.size) nextEventId + 1 else nextEventId,
            )
        }
    }
    "question_resolved" -> {
        val id = event.str("id")?.takeIf { it.isNotBlank() }
        val askId = event.str("askId")?.takeIf { it.isNotBlank() }
        val answers = runCatching {
            event["answers"]?.jsonArray?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }
        }.getOrNull().orEmpty()
        val next = resolveQuestions(feed, id, askId, answers, event.bool("dismissed"))
            ?: if (id == null && askId == null) dismissPendingQuestions(feed) else null
        copy(feed = next ?: feed, semanticQuestions = true).releaseResolvedLocks()
    }
    "answer_ack" -> {
        val id = event.str("id")?.takeIf { it.isNotBlank() }
        if (id == null) {
            this
        } else {
            // The desktop injected the answer: the card stays locked on EVERY
            // viewer (not just the sender) and a stepper advances a step.
            copy(
                answerLocks = answerLocks + (id to AnswerState.Acked),
                semanticQuestions = true,
            )
        }
    }
    "subagent" -> {
        val subagentId = event.str("id")?.takeIf { it.isNotBlank() }
        val detail = event.str("detail")?.takeIf { it.isNotBlank() }
        val completed = event.str("status") == "completed"
        val closed = if (subagentId != null && completed) {
            completeSubagent(feed, subagentId, detail)
        } else {
            null
        }
        when {
            subagentId == null -> this
            closed != null -> withFeed(closed)
            else -> append(
                AgentFeedItem.Subagent(
                    id = nextEventId,
                    subagentId = subagentId,
                    agentType = event.str("agentType")?.takeIf { it.isNotBlank() } ?: "agent",
                    completed = completed,
                    detail = detail,
                ),
            )
        }
    }
    "permission" -> {
        val tool = event.str("tool")?.takeIf { it.isNotBlank() }
        if (tool == null) {
            this
        } else {
            append(
                AgentFeedItem.Permission(
                    id = nextEventId,
                    tool = tool,
                    detail = event.str("detail")?.takeIf { it.isNotBlank() },
                ),
            )
        }
    }
    else -> this
}

/** Lock a card the instant its answer goes out — no double-tap (EXP-249). */
fun ActivityFeedState.lockAnswer(
    lockKey: String,
    labels: List<String> = emptyList(),
): ActivityFeedState = copy(
    answerLocks = answerLocks + (lockKey to AnswerState.Sending),
    answerLabels = if (labels.isEmpty()) answerLabels - lockKey else answerLabels + (lockKey to labels),
)

/** The locally picked labels of a locked card, joined for display; null when
 *  nothing is locked under [lockKey] or no label was recorded (EXP-588). */
fun ActivityFeedState.localAnswerSummary(lockKey: String): String? {
    if (!answerLocks[lockKey].locksCard()) return null
    return answerLabels[lockKey]?.takeIf { it.isNotEmpty() }?.joinToString(", ")
}

/** Flip a lock whose `answer_ack` never arrived to [AnswerState.Failed]: the
 *  card is answerable again AND says why it re-surfaced (EXP-334 — the silent
 *  unlock read as the stepper inexplicably jumping back). An acknowledged
 *  card stays locked. */
fun ActivityFeedState.failUnacknowledged(lockKey: String): ActivityFeedState =
    if (answerLocks[lockKey] == AnswerState.Sending) {
        copy(
            answerLocks = answerLocks + (lockKey to AnswerState.Failed),
            answerLabels = answerLabels - lockKey,
        )
    } else {
        this
    }

/** Drop the locks of cards that have since resolved — a resolved card renders
 *  its answer instead of options, so the lock guards nothing. */
fun ActivityFeedState.releaseResolvedLocks(): ActivityFeedState {
    val done = feed.filterIsInstance<AgentFeedItem.Question>()
        .filter { it.resolved }
        .map { questionLockKey(it) }
        .toSet()
    val next = answerLocks - done
    return if (next.size == answerLocks.size) this else copy(answerLocks = next)
}

/** A locally-echoed steered message, shown before its transcript twin. */
fun ActivityFeedState.appendUserMessage(text: String): ActivityFeedState =
    append(AgentFeedItem.UserMessage(nextEventId, text))

private fun ActivityFeedState.append(item: AgentFeedItem): ActivityFeedState =
    copy(feed = capFeed(feed + item), nextEventId = nextEventId + 1)

private fun ActivityFeedState.withFeed(next: List<AgentFeedItem>): ActivityFeedState =
    copy(feed = capFeed(next))

/** Trim to the client cap — the oldest events fall off the top. */
fun capFeed(feed: List<AgentFeedItem>): List<AgentFeedItem> =
    if (feed.size > FEED_CAP) feed.takeLast(FEED_CAP) else feed

sealed interface AgentPhase {
    data object Idle : AgentPhase
    data object Connecting : AgentPhase
    data object Live : AgentPhase

    /**
     * The relay reported no_such_session while the synced row still says
     * running — the desktop is still dialing its publisher socket. The VM
     * auto-redials (fresh ticket) every ~3s until the room is live.
     */
    data object Starting : AgentPhase

    /** The session ended (relay `bye`, or the synced row flipped to ended). */
    data class Ended(val detail: String? = null) : AgentPhase

    /** Unexpected socket loss. With [reconnecting] the VM auto-redials on
     *  jittered exponential backoff (EXP-243 — no manual Reconnect button);
     *  false only for terminal states (steer disabled on this instance). */
    data class Closed(val detail: String? = null, val reconnecting: Boolean = false) : AgentPhase
}
