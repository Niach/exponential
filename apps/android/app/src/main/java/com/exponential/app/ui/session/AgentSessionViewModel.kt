package com.exponential.app.ui.session

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.decodeSteerTicketPerm
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.http.HttpStatusCode
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import javax.inject.Inject
import kotlin.math.pow
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the mobile
// "Agent session" chat view; apps/steer-relay/src/protocol.ts): the socket
// joins with {"t":"join","channel":"activity"} and receives scrubbed
// {t:'activity', event} frames (narration / tool headlines / questions /
// subagents / worktree diffs). The PTY mirror is gone (EXP-249) — a stray
// BINARY frame from an old desktop is ignored. Steering is message-shaped and
// fully seamless (EXP-312 — no operator claim; the relay gates on the ticket's
// steer perm): chunked input + a separate \r; answers are semantic
// {t:'answer'} frames, with the raw-keystroke path kept only for question
// cards published by pre-EXP-249 desktops (no wire id).

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
private const val INPUT_CHUNK_CHARS = 4096

/** Client-side feed cap — matches the relay's ACTIVITY_LOG_CAP (EXP-249). */
const val FEED_CAP = 2000

/** How long an unacknowledged answer keeps its card locked (EXP-249): the
 *  desktop confirms injection with `answer_ack`, and a silently dropped frame
 *  must not strand the card as un-answerable forever. */
private const val ANSWER_ACK_TIMEOUT_MS = 5_000L

/** Redial cadence while the desktop's publisher socket is still starting. */
private const val STARTING_RETRY_MS = 3_000L

/** Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
 *  exponential 3s→30s, mirroring the web viewer's starting retry — the jitter
 *  desyncs a herd of viewers all foregrounding at once. */
private const val RECONNECT_BASE_MS = 3_000L
private const val RECONNECT_MAX_MS = 30_000L

/** Shown when the session's row no longer exists — a swept row (or one that
 *  left this client's sync scope) is over as far as any client can tell, and
 *  nothing about it is retryable. */
private const val SESSION_GONE_DETAIL = "This session is no longer available."

/** Echo-FIFO bounds (EXP-78): a mid-turn steered message can take a while to
 *  hit the transcript, but an unmatched echo must not swallow an identical
 *  message sent much later. */
private const val ECHO_CAP = 8
private const val ECHO_TTL_MS = 300_000L

/** One answer choice of a [AgentFeedItem.Question] — `key` is the raw
 *  keystroke that selects it in the desktop TUI picker (mapped desktop-side)
 *  and the value echoed back in a semantic `answer` frame. */
data class QuestionOption(
    val label: String,
    val key: String,
    val description: String? = null,
)

/** This client's send state for one question card (EXP-249): the card locks
 *  the instant an answer goes out and stays locked through the desktop's
 *  `answer_ack`; a missing ack unlocks it after [ANSWER_ACK_TIMEOUT_MS] so the
 *  answer can be retried. */
enum class AnswerState { Sending, Acked }

/** One rendered feed entry. Diffs never enter the feed — see [AgentSessionViewModel.latestDiff]. */
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

    data class SubagentRun(
        val subagent: AgentFeedItem.Subagent,
        val tools: List<AgentFeedItem.Tool>,
    ) : AgentFeedRow {
        override val id get() = subagent.id
    }
}

/** Render-time projection of the flat feed — a pure function: the feed itself
 *  (and [activeQuestionIds] over it) is never restructured.
 *  - a subagent row absorbs the tool calls that follow it,
 *  - all question cards sharing an askId collapse into ONE stepper row,
 *    anchored where the ask's first card landed,
 *  - runs of ≥2 consecutive tool calls collapse into one "N tool calls" row. */
fun groupFeedRows(feed: List<AgentFeedItem>): List<AgentFeedRow> {
    val stepsByAsk = feed.filterIsInstance<AgentFeedItem.Question>()
        .filter { it.askId != null }
        .groupBy { it.askId!! }
    val emittedAsks = mutableSetOf<String>()
    val rows = mutableListOf<AgentFeedRow>()
    var i = 0
    while (i < feed.size) {
        val item = feed[i]
        when {
            item is AgentFeedItem.Subagent -> {
                var end = i
                while (end + 1 < feed.size) {
                    val next = feed[end + 1]
                    if (next is AgentFeedItem.Tool && next.subagentId == item.subagentId) end++ else break
                }
                rows.add(
                    AgentFeedRow.SubagentRun(
                        item,
                        feed.subList(i + 1, end + 1).map { it as AgentFeedItem.Tool },
                    ),
                )
                i = end + 1
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
                while (end + 1 < feed.size && feed[end + 1] is AgentFeedItem.Tool) end++
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
            else -> append(AgentFeedItem.Narration(nextEventId, text))
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
                QuestionOption(label, key, option.str("description")?.takeIf { it.isNotBlank() })
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
fun ActivityFeedState.lockAnswer(lockKey: String): ActivityFeedState =
    copy(answerLocks = answerLocks + (lockKey to AnswerState.Sending))

/** Release a lock whose `answer_ack` never arrived, so the answer can be
 *  retried; an acknowledged card stays locked. */
fun ActivityFeedState.unlockUnacknowledged(lockKey: String): ActivityFeedState =
    if (answerLocks[lockKey] == AnswerState.Sending) copy(answerLocks = answerLocks - lockKey)
    else this

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

@HiltViewModel
class AgentSessionViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    private val client: HttpClient,
    private val json: Json,
) : ViewModel() {

    val codingSessionId: String = savedStateHandle["codingSessionId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced coding_sessions row — flips to ended via Electric. */
    val session: StateFlow<CodingSessionEntity?> =
        dbFlow.scopedQuery<CodingSessionEntity?>(null) {
            it.codingSessionDao().observeById(codingSessionId)
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val currentUserId: StateFlow<String?> = auth.userId

    private val _phase = MutableStateFlow<AgentPhase>(AgentPhase.Idle)
    val phase: StateFlow<AgentPhase> = _phase

    private val _perm = MutableStateFlow("view")
    val perm: StateFlow<String> = _perm

    // The feed survives reconnects and the session end — only a fresh screen
    // (new VM) or the relay's activity_reset starts it empty (EXP-249).
    private val _activity = MutableStateFlow(ActivityFeedState())
    val activity: StateFlow<ActivityFeedState> = _activity

    /** A failed [killSession] call's message (EXP-268) — rendered as a banner. */
    private val _killError = MutableStateFlow<String?>(null)
    val killError: StateFlow<String?> = _killError

    /** Pending ack deadlines, one per locked card. */
    private val ackTimeouts = mutableMapOf<String, Job>()

    /** Locally-echoed sent messages awaiting their transcript-derived
     *  `user_message` event (EXP-78 dedupe): text → sent-at millis. */
    private val recentEchoes = ArrayDeque<Pair<String, Long>>()
    private var ws: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null

    /** Consecutive failed reconnect dials — indexes the backoff curve; reset
     *  on a successful (live) connection and on an explicit connect(). */
    private var reconnectAttempts = 0

    /** Set once the synced row has actually been observed — [session] starts
     *  null (nothing loaded yet), so only a null AFTER a real row proves the
     *  row is gone rather than still on its way. */
    private var sawSessionRow = false

    /** Auto-connect once when the screen opens; drops after that
     *  auto-reconnect (EXP-243). */
    fun connectIfIdle() {
        if (_phase.value == AgentPhase.Idle) connect()
    }

    fun connect() {
        connectJob?.cancel()
        reconnectAttempts = 0
        connectJob = viewModelScope.launch {
            while (isActive) {
                when (val outcome = dialOnce()) {
                    DialOutcome.RetryStarting -> {
                        // The desktop hasn't published the room yet. Keep
                        // redialing (fresh ticket each time) while the synced
                        // row still says running.
                        _phase.value = AgentPhase.Starting
                        delay(STARTING_RETRY_MS)
                        if (sessionIsOver()) {
                            _phase.value = AgentPhase.Ended()
                            return@launch
                        }
                    }
                    is DialOutcome.Ended -> {
                        _phase.value = AgentPhase.Ended(outcome.detail)
                        return@launch
                    }
                    is DialOutcome.Closed -> {
                        if (!outcome.retryable) {
                            _phase.value = AgentPhase.Closed(outcome.detail)
                            return@launch
                        }
                        // Never park on a dead socket behind a manual button
                        // (EXP-243) — auto-redial on backoff; the phase
                        // carries the reconnecting flag so the UI shows
                        // "Reconnecting…" instead of a Reconnect action.
                        _phase.value = AgentPhase.Closed(outcome.detail, reconnecting = true)
                        delay(reconnectDelayMs(reconnectAttempts++))
                        if (sessionIsOver()) {
                            _phase.value = AgentPhase.Ended()
                            return@launch
                        }
                    }
                }
            }
        }
    }

    /** Foreground revival (EXP-243): skip any pending reconnect backoff and
     *  redial immediately; while nominally live, ping-probe the socket so a
     *  connection that died in the background surfaces (and auto-reconnects)
     *  now instead of on the next failed read. */
    fun reconnectNow() {
        val p = _phase.value
        if (p is AgentPhase.Closed && p.reconnecting) {
            connect()
        } else if (p == AgentPhase.Live) {
            ws?.outgoing?.trySend(Frame.Ping(ByteArray(0)))
        }
    }

    /**
     * Whether the session is over as far as this client can tell: an
     * explicitly `ended` row, or a row that VANISHED (stale coding_sessions
     * rows get swept, and a row can also leave this client's sync scope). A
     * deleted row can never report `ended` itself, and that status is the only
     * other exit from the retry loops — so treating its disappearance as
     * "still running" would keep them dialing a session that no longer exists.
     */
    private fun sessionIsOver(): Boolean {
        val row = session.value
        if (row != null) {
            sawSessionRow = true
            return row.status == DomainContract.codingSessionStatusEnded
        }
        return sawSessionRow
    }

    /** Equal-jitter exponential backoff (web parity): half the capped
     *  exponential delay fixed, half random. */
    private fun reconnectDelayMs(attempt: Int): Long {
        val capped = minOf(
            RECONNECT_MAX_MS.toDouble(),
            RECONNECT_BASE_MS * 2.0.pow(attempt),
        )
        return (capped / 2 + Random.nextDouble() * (capped / 2)).toLong()
    }

    private sealed interface DialOutcome {
        /** no_such_session while the synced row says running — auto-retry. */
        data object RetryStarting : DialOutcome
        data class Ended(val detail: String? = null) : DialOutcome
        data class Closed(val detail: String? = null, val retryable: Boolean = true) : DialOutcome
    }

    private suspend fun dialOnce(): DialOutcome {
        // Hold the Starting / reconnecting-Closed phase steady across
        // auto-retry redials — flipping to Connecting per attempt made the
        // header flicker every ~3s while the desktop was still dialing its
        // publisher (and would flicker the reconnect banner the same way).
        val held = _phase.value
        if (held != AgentPhase.Starting && !(held is AgentPhase.Closed && held.reconnecting)) {
            _phase.value = AgentPhase.Connecting
        }

        // `bye` / no_such_session must win over the generic close handler.
        var sawEnd = false
        var retryStarting = false
        var detail: String? = null
        // A server "no" that can never turn into a yes — wins over everything.
        var terminal: DialOutcome? = null

        var socket: DefaultClientWebSocketSession? = null
        try {
            val accountId = auth.activeAccountId.value
                ?: throw IllegalStateException("No active account")
            val minted = steerApi.mintViewerTicket(accountId, codingSessionId)
            if (!minted.isUsable) {
                // Config state, not a transient failure — retrying can't help.
                return DialOutcome.Closed("Live sessions are unavailable on this instance.", retryable = false)
            }
            _perm.value = decodeSteerTicketPerm(minted.ticket!!)

            // The server-returned url is the full ws(s)://…/ws?ticket=… dial URL.
            socket = client.webSocketSession(urlString = minted.url!!)
            ws = socket
            // The feed is NOT wiped here (EXP-249): the relay sends an explicit
            // {t:'activity_reset'} immediately before replaying the room's log,
            // so a dial that never reaches a replay leaves the visible history
            // alone. After a reconnect the replayed transcript event is the
            // ONLY copy of a sent message — no stale echo may swallow it.
            recentEchoes.clear()
            socket.send(Frame.Text("""{"t":"join","channel":"activity"}"""))
            // NOT Live yet — the relay may answer the join with no_such_session
            // (desktop still starting). The phase flips to Live on the first
            // confirming server frame instead (the relay sends activity_reset
            // immediately on a successful join), so the Starting retry loop
            // never flashes the Live header/composer/empty state.

            for (frame in socket.incoming) {
                when (frame) {
                    // The PTY mirror is gone (EXP-249); an old desktop's binary
                    // output frames are silently dropped.
                    is Frame.Binary -> Unit
                    is Frame.Text -> {
                        val result = handleControlFrame(frame.readText())
                        if (result != null) {
                            if (result.live && _phase.value != AgentPhase.Live) {
                                _phase.value = AgentPhase.Live
                                reconnectAttempts = 0
                            }
                            sawEnd = sawEnd || result.sawEnd
                            result.detail?.let { detail = it }
                            if (result.retryStarting) {
                                retryStarting = true
                                break
                            }
                        }
                    }
                    else -> Unit
                }
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            if (detail == null) {
                detail = trpcErrorMessage(t, t.message ?: "Connection failed")
            }
            // Only the mint throws TrpcException here, and two of its codes are
            // permanent: NOT_FOUND (404) means the coding_sessions row is gone
            // — the mint will refuse forever and the deleted row can never
            // report `ended`, the loop's only other exit — and FORBIDDEN (403)
            // means access to the session was revoked. Retrying either parks
            // the screen at the 30s backoff cap behind a raw error string for
            // as long as it stays open.
            terminal = when {
                t !is TrpcException -> null
                t.status == HttpStatusCode.NotFound -> DialOutcome.Ended(SESSION_GONE_DETAIL)
                t.status == HttpStatusCode.Forbidden -> DialOutcome.Closed(detail, retryable = false)
                else -> null
            }
        } finally {
            ws = null
            runCatching { socket?.cancel() }
        }

        terminal?.let { return it }
        return when {
            sawEnd -> DialOutcome.Ended(detail)
            // Heartbeat-stale rows don't warrant a redial (EXP-153) — the row
            // is a phantom, not a session that's still starting.
            retryStarting && session.value?.let { CodingSessionLiveness.isLive(it) } == true ->
                DialOutcome.RetryStarting
            else -> DialOutcome.Closed(detail)
        }
    }

    private data class FrameResult(
        val sawEnd: Boolean = false,
        val detail: String? = null,
        val retryStarting: Boolean = false,
        /** The frame proves the join succeeded — the room is live on the relay. */
        val live: Boolean = false,
    )

    private fun handleControlFrame(raw: String): FrameResult? {
        val obj = runCatching { json.parseToJsonElement(raw).jsonObject }.getOrNull() ?: return null
        return when ((obj["t"] as? JsonPrimitive)?.contentOrNull) {
            "activity" -> {
                handleActivityEvent(obj["event"] as? JsonObject)
                FrameResult(live = true)
            }
            // The relay's uniform "clear your feed now" signal (EXP-249) — sent
            // to every activity viewer right before its join replay, and fanned
            // out whenever the publisher resets the log. It is the ONLY thing
            // that wipes the feed.
            "activity_reset" -> {
                resetActivity()
                FrameResult(live = true)
            }
            "bye" -> {
                val outcome = (obj["outcome"] as? JsonPrimitive)?.contentOrNull
                if (outcome == "publisher_lost") {
                    // The desktop's relay socket dropped but the session may
                    // still be running — the synced row is the truth. Stay
                    // retryable (Closed, auto-reconnecting).
                    FrameResult(
                        detail = "The desktop's connection to the relay dropped — waiting for it to come back.",
                    )
                } else {
                    FrameResult(sawEnd = true, detail = outcome?.takeIf { it != "ended" })
                }
            }
            "error" -> {
                val code = (obj["code"] as? JsonPrimitive)?.contentOrNull
                if (code == "no_such_session") {
                    // Not live on the relay (yet). With the synced row still
                    // running this flips into the auto-retrying Starting phase.
                    FrameResult(
                        detail = "The live stream isn't up yet — the desktop may still be connecting.",
                        retryStarting = true,
                    )
                } else {
                    FrameResult(
                        detail = (obj["message"] as? JsonPrimitive)?.contentOrNull ?: code,
                    )
                }
            }
            else -> null // input/answer/kill/legacy presence — not activity-viewer-relevant
        }
    }

    private fun handleActivityEvent(event: JsonObject?) {
        if (event == null) return
        val before = _activity.value
        val after = before.applyActivityEvent(event) { consumeEcho(it) }
        if (after === before) return
        // A settled lock — acknowledged, or released by its card resolving —
        // has no pending deadline left to guard.
        ackTimeouts.keys.toList().forEach { key ->
            if (after.answerLocks[key] != AnswerState.Sending) ackTimeouts.remove(key)?.cancel()
        }
        _activity.value = after
    }

    /** Wipe everything the relay's activity log owns (EXP-249). */
    private fun resetActivity() {
        recentEchoes.clear()
        ackTimeouts.values.forEach { it.cancel() }
        ackTimeouts.clear()
        _activity.value = ActivityFeedState()
    }

    /** Whether an incoming `user_message` matches a recent local echo —
     *  consumes the matched entry (and evicts expired ones); true = skip it. */
    private fun consumeEcho(text: String): Boolean {
        val now = System.currentTimeMillis()
        recentEchoes.removeAll { now - it.second > ECHO_TTL_MS }
        val needle = text.trim()
        val match = recentEchoes.firstOrNull { it.first == needle } ?: return false
        recentEchoes.remove(match)
        return true
    }

    // ── Steering (message-shaped; relay gates on the ticket's steer perm) ────

    /**
     * Send one message to the agent: the text (chunked ≤4 KiB), then a
     * SEPARATE `\r` frame — bundled into one write TUI apps treat the
     * trailing return as a paste, which inserts instead of submitting.
     */
    fun sendMessage(text: String) {
        if (text.isEmpty() || _perm.value != "steer") return
        val socket = ws ?: return
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        recentEchoes.addLast(text.trim() to System.currentTimeMillis())
        while (recentEchoes.size > ECHO_CAP) recentEchoes.removeFirst()
        _activity.value = _activity.value.appendUserMessage(text)
        viewModelScope.launch {
            runCatching {
                var i = 0
                while (i < text.length) {
                    val chunk = text.substring(i, minOf(i + INPUT_CHUNK_CHARS, text.length))
                    val frame = buildJsonObject {
                        put("t", "input")
                        put("data", chunk)
                    }
                    socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
                    i += INPUT_CHUNK_CHARS
                }
                socket.send(Frame.Text("""{"t":"input","data":"\r"}"""))
            }
        }
    }

    /**
     * Answer a question card that carries a wire id (EXP-249): ONE semantic
     * `answer` frame — the desktop owns the mapping onto its TUI and confirms
     * the injection with `answer_ack`. The card locks the moment the frame
     * goes out (no double-tap) and unlocks only if nothing comes back within
     * [ANSWER_ACK_TIMEOUT_MS].
     */
    fun sendQuestionAnswer(questionId: String, askId: String?, keys: List<String>) {
        if (keys.isEmpty() || _perm.value != "steer") return
        if (_activity.value.answerLocks.containsKey(questionId)) return
        val socket = ws ?: return
        lockAnswer(questionId)
        viewModelScope.launch {
            runCatching {
                val frame = buildJsonObject {
                    put("t", "answer")
                    put("questionId", questionId)
                    if (askId != null) put("askId", askId)
                    putJsonArray("keys") { keys.forEach { add(JsonPrimitive(it)) } }
                }
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
            }
        }
    }

    /**
     * Answer a question card published by a pre-EXP-249 desktop (no wire id):
     * the option's raw TUI keystroke. The digit ALONE — bundling
     * a `\r` submitted the picker AND cascaded into whatever picker claude
     * opened next. [lock] is off for multi-select toggles, which tap repeatedly
     * before [sendSubmit] advances the picker.
     */
    fun sendLegacyAnswer(lockKey: String, key: String, lock: Boolean = true) {
        if (key.isEmpty() || _perm.value != "steer") return
        if (lock && _activity.value.answerLocks.containsKey(lockKey)) return
        val socket = ws ?: return
        if (lock) lockAnswer(lockKey)
        viewModelScope.launch {
            runCatching {
                val frame = buildJsonObject {
                    put("t", "input")
                    put("data", key)
                }
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
            }
        }
    }

    private fun lockAnswer(lockKey: String) {
        _activity.value = _activity.value.lockAnswer(lockKey)
        ackTimeouts.remove(lockKey)?.cancel()
        ackTimeouts[lockKey] = viewModelScope.launch {
            delay(ANSWER_ACK_TIMEOUT_MS)
            ackTimeouts.remove(lockKey)
            // Nothing came back — unlock so the answer can be sent again.
            _activity.value = _activity.value.unlockUnacknowledged(lockKey)
        }
    }

    /**
     * Kill the session (EXP-268): tRPC `steer.killSession` flips the synced
     * row to `ended` (which this screen already reacts to) and best-effort
     * kills the live terminal through the relay — so no local state change on
     * success; a failure surfaces via [killError].
     */
    fun killSession() {
        viewModelScope.launch {
            _killError.value = null
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                steerApi.killSession(accountId, codingSessionId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _killError.value = trpcErrorMessage(t, "Couldn't kill the session")
            }
        }
    }

    /** Advance a legacy multi-select question. Tab, NOT Enter: with the cursor
     *  on an option row Enter TOGGLES it (verified against claude v2.1.215 —
     *  silently corrupting the selection), while Tab moves to the next
     *  tab/review step, whose picker the grid watcher publishes as its own
     *  card (EXP-197). */
    fun sendSubmit() = sendLegacyAnswer("", "\t", lock = false)

    override fun onCleared() {
        connectJob?.cancel()
        super.onCleared()
    }
}
