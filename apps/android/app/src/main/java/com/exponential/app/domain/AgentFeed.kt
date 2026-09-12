package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.jsonArray

// The activity feed's model and its pure reducer — the "Agent session" chat
// view's whole vocabulary (EXP-32/EXP-249), compose-free and unit testable on
// its own. It lives in `domain` because BOTH sides need it: the socket in
// `data/steer` folds events into it, the screen in `ui/session` renders the
// result — mirroring iOS's ExpCore `AgentFeed.swift` and web's
// `lib/agent-feed.ts`.

/** EXP-783: the transcript keeps the WHOLE run — the session screen paints a
 *  WINDOW over it ([FEED_WINDOW]) and grows it upward, so keeping everything
 *  costs nothing per frame. These are safety ceilings on the app's memory, not
 *  a display cap, and they are sized to the device journal (JOURNAL_FILE_CAP)
 *  because that file is exactly what a replay reads back. The relay's
 *  ACTIVITY_LOG_CAP is deliberately NOT matched any more: it bounds the tail a
 *  joining viewer replays, and older pages are asked for (`history_page`).
 *
 *  Every number is the contract's `steerFeed` section (EXP-795): web, iOS and
 *  the desktop read the same generated constants, so the budgets, the trim
 *  target and the window cannot drift per client. */
const val FEED_BYTE_CAP = DomainContract.steerFeedByteCap

/** The companion item ceiling — a run of tiny events would sit far under the
 *  byte budget while costing a list slot each. */
const val FEED_ITEM_CAP = DomainContract.steerFeedItemCap

/** How many of the run's newest rows the session screen renders, and how much
 *  older transcript one "Load earlier" pulls in. */
const val FEED_WINDOW = DomainContract.steerFeedWindow
const val FEED_WINDOW_STEP = DomainContract.steerFeedWindowStep

/** EXP-783: events per `history_page` ask — the relay's schema rejects
 *  anything larger. */
const val HISTORY_PAGE_LIMIT = DomainContract.steerFeedHistoryPageMax

/** What one row weighs against [FEED_BYTE_CAP]: the text it carries plus a
 *  flat per-item overhead standing in for the object itself. An estimate on
 *  purpose — this budget bounds memory, it does not account for it. */
fun feedItemBytes(item: AgentFeedItem): Long {
    val overhead = DomainContract.steerFeedItemOverheadBytes
    return overhead + when (item) {
        is AgentFeedItem.Narration -> item.text.length.toLong()
        is AgentFeedItem.UserMessage -> item.text.length.toLong()
        // EXP-786: a folded per-call diff weighs too.
        is AgentFeedItem.Tool ->
            (
                item.name.length + (item.detail?.length ?: 0) + (item.diff?.length ?: 0) +
                    // EXP-846: the folded Exponential-tool preview weighs too.
                    (item.preview?.weight() ?: 0)
                ).toLong()
        is AgentFeedItem.Permission -> (item.tool.length + (item.detail?.length ?: 0)).toLong()
        is AgentFeedItem.Subagent ->
            (
                item.subagentId.length + item.agentType.length + (item.detail?.length ?: 0) +
                    (item.title?.length ?: 0)
                ).toLong()
        // EXP-856: the warning row carries the engine's sentence verbatim.
        is AgentFeedItem.DuplicateAgent ->
            (item.subagentId.length + item.detail.length + (item.title?.length ?: 0)).toLong()
        is AgentFeedItem.Question ->
            (item.text.length + (item.answer?.length ?: 0) + (item.header?.length ?: 0) +
                item.options.sumOf { it.label.length + it.key.length }).toLong()
        is AgentFeedItem.Compaction -> 0L
    }
}

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

    /** EXP-783: the publisher's monotonic index for the event behind this row,
     *  when it sent one. The only monotonic anchor on the wire — it is what
     *  lets a join replay be spliced onto a transcript prefix already on
     *  screen, and what an older-page request is addressed relative to. */
    val seq: Long?

    /** Agent prose.
     *
     *  [messageId] (EXP-772) is the ACP id of the assistant message this
     *  chunk came out of: the engine flushes one message in several events,
     *  and consecutive flushes of the SAME message grow ONE row instead of a
     *  column of fragments (see `mergeNarration`).
     *
     *  [subagentId] (EXP-773) tags prose a subagent wrote — it renders inside
     *  that subagent's run, never in the main feed. */
    data class Narration(
        override val id: Long,
        val text: String,
        val messageId: String? = null,
        val subagentId: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem

    /** EXP-785: [callId] is the ACP tool-call id a later `tool_update` folds
     *  into this row by (null on rows from a pre-EXP-785 publisher, which
     *  then never settle); [toolKind] is ACP's kind bucket (a contract
     *  `toolKind` value); [settled] = a status landed (the call ENDED),
     *  [failed] = that status was `failed` (a later `completed` clears it).
     *  EXP-786: [diff] is the per-call unified diff an `edit` published,
     *  already cut to the contract's caps by the publisher.
     *  EXP-846: [preview] is what an EXPONENTIAL MCP call returned, folded in
     *  by its `tool_update` — plumbed here in phase 1, rendered later. */
    data class Tool(
        override val id: Long,
        val name: String,
        val detail: String?,
        /** Set when the call came from a subagent (EXP-249) — the row renders
         *  inside that subagent's group, not in the main feed. */
        val subagentId: String? = null,
        override val seq: Long? = null,
        val callId: String? = null,
        val toolKind: String? = null,
        val settled: Boolean = false,
        val failed: Boolean = false,
        val diff: String? = null,
        val preview: ToolResultPreview? = null,
    ) : AgentFeedItem

    /** A human turn (EXP-78): the initial prompt or a steered message.
     *  [subagentId] (EXP-773) tags a turn addressed to a subagent — same
     *  scoping rule as [Narration]. */
    data class UserMessage(
        override val id: Long,
        val text: String,
        val subagentId: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem

    /** An interactive question (AskUserQuestion / plan approval, EXP-78).
     *  [planMode] marks an ExitPlanMode plan-approval picker (EXP-97) —
     *  presentation-only, absent on events from older desktops/relays.
     *  [resolved]/[answer] come from the desktop's `question_resolved` event
     *  (EXP-249) — a resolved card renders its answer and is never answerable
     *  again. */
    data class Question(
        override val id: Long,
        val text: String,
        val options: List<QuestionOption>,
        val multiSelect: Boolean,
        val planMode: Boolean = false,
        val resolved: Boolean = false,
        val answer: String? = null,
        /** EXP-820: retired by a DISMISSAL (`session/cancel`, an interrupt) —
         *  the ask is over without an answer, so a stepper stops waiting. */
        val dismissed: Boolean = false,
        /** Stable wire identity (EXP-249) and the key of the card's answer
         *  lock: the semantic `answer` frame names it, and a re-emission of
         *  it replaces the card in place. Every publisher stamps one, and a
         *  frame without one is dropped at decode. */
        val wireId: String,
        /** Groups the steps of one multi-question ask into a stepper card. */
        val askId: String? = null,
        /** 1-based step position; absent on the ask's final submit step. */
        val index: Int? = null,
        val total: Int? = null,
        val header: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem {
        /** The ask's final review/submit step: it belongs to an ask but
         *  carries no step position of its own, and consumes no answer. */
        val isSubmitStep: Boolean get() = askId != null && index == null
    }

    /** A subagent's lifecycle row (EXP-249) — the header of the collapsible
     *  group its tool calls render inside.
     *
     *  EXP-748: [toolCalls] is the PUBLISHER's count of the calls this
     *  subagent made, stamped on the completed edge. The replay log evicts a
     *  subagent's tool events first, so the visible rows can undercount long
     *  after the fact while this number stays honest. Absent on an older
     *  desktop.
     *
     *  EXP-847: [title] is what the spawning Agent call DESCRIBED the run as
     *  ("Find the regression in the sync loop") — the label every client
     *  prefers over [agentType], which stays the secondary caption. Absent on
     *  a publisher older than EXP-847 and on a call that described nothing. */
    data class Subagent(
        override val id: Long,
        val subagentId: String,
        val agentType: String,
        val completed: Boolean,
        val detail: String? = null,
        val toolCalls: Int? = null,
        val title: String? = null,
        /** EXP-850 (S4): this agent belongs to that workflow card — its edges
         *  nest under the card and it is never offered as a steerable tab. */
        val workflowId: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem

    /** EXP-856: a SECOND copy of an agent that is still running started under
     *  the same id — `SendMessage` to a live agent resumes it from its
     *  transcript, and that copy edits the same files as the original. The
     *  engine writes the whole sentence ([detail]); every client renders it
     *  verbatim as an amber warning row, under the workflow card when
     *  [workflowId] is set and inline otherwise. */
    data class DuplicateAgent(
        override val id: Long,
        val subagentId: String,
        val detail: String,
        val title: String? = null,
        val workflowId: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem

    /** A permission prompt the agent hit (EXP-249) — informational only: it is
     *  answered on the desktop, never from here. */
    data class Permission(
        override val id: Long,
        val tool: String,
        val detail: String? = null,
        override val seq: Long? = null,
    ) : AgentFeedItem

    /** EXP-724: the quiet "Context compacted" divider a `compaction ended`
     *  leaves in the timeline — the STRIP itself is
     *  [ActivityFeedState.compacting], never a row. */
    data class Compaction(
        override val id: Long,
        override val seq: Long? = null,
    ) : AgentFeedItem
}

/** EXP-783: the same row under a new feed id. Used only by the older-page
 *  prepend, which folds a page through the ordinary reducer (ids from zero)
 *  and then renumbers it BELOW everything on screen, so no visible row's
 *  identity changes. */
fun AgentFeedItem.withId(id: Long): AgentFeedItem = when (this) {
    is AgentFeedItem.Narration -> copy(id = id)
    is AgentFeedItem.Tool -> copy(id = id)
    is AgentFeedItem.UserMessage -> copy(id = id)
    is AgentFeedItem.Question -> copy(id = id)
    is AgentFeedItem.Subagent -> copy(id = id)
    is AgentFeedItem.DuplicateAgent -> copy(id = id)
    is AgentFeedItem.Permission -> copy(id = id)
    is AgentFeedItem.Compaction -> copy(id = id)
}

/** The subagent a feed item belongs to, if any — the grouping key of a
 *  subagent run. EXP-773 widened it past tool calls: prose and human turns
 *  carry it too, so a subagent's whole conversation groups under its own row.
 *  The lifecycle markers are matched separately (they ARE the row). */
fun AgentFeedItem.subagentKey(): String? = when (this) {
    is AgentFeedItem.Tool -> subagentId
    is AgentFeedItem.Narration -> subagentId
    is AgentFeedItem.UserMessage -> subagentId
    else -> null
}

/** EXP-724: the caption of the indeterminate strip pinned above the composer
 *  while [ActivityFeedState.compacting] is set — byte-identical to web, iOS
 *  and the desktop (`COMPACTING_LABEL`); move all four in lockstep. */
const val COMPACTING_LABEL = "Compacting context…"

/** The marker row [AgentFeedItem.Compaction] renders — byte-identical ×4. */
const val COMPACTED_LABEL = "Context compacted"

/** A lone `compaction started` (its `ended` lost, or a stale replayed start)
 *  drops the strip after this long. Web `COMPACTION_TIMEOUT_MS` / iOS /
 *  desktop `COMPACTION_TIMEOUT` parity. The CONNECTION arms the timer. */
const val COMPACTION_TIMEOUT_MS = 180_000L

/** `subagent.agentType` when the desktop's hook payload carried none — old
 *  desktop builds also stamp it onto the COMPLETED edge, so it is a sentinel
 *  the label selection skips past, never a type to prefer (EXP-350). */
const val SUBAGENT_FALLBACK_TYPE = "agent"

// ── EXP-746 steering v2: the agent's live configuration ─────────────────────
//
// `config_state` and `usage` are LATEST-WINS STATE, never feed rows (the
// `latestDiff` / `compacting` precedent): the relay keeps only the newest of
// each per room and replays it right after the log, so a joining viewer paints
// its chips from ONE frame. Mirrored ×4 — web `agent-feed.ts`
// SessionConfigState/SessionUsageState, iOS `AgentSessionConfig`/
// `AgentSessionUsage`, desktop `feed.rs` SessionConfig/SessionUsage.

/** One selectable value of a [ConfigOption] (or of the mode chip). */
data class ConfigValue(val id: String, val label: String)

/** One live option the publisher advertises — model, effort, a toggle. */
data class ConfigOption(
    val id: String,
    val label: String,
    /** Grouping hint (`model`, `effort`, …); unknown ones still render. */
    val category: String? = null,
    /** In force right now; BLANK is the CLI's own default, absent is unknown. */
    val value: String? = null,
    /** Empty = read-only on this run: render the value, offer no menu. */
    val values: List<ConfigValue> = emptyList(),
)

/** One agent mode (`plan`, `auto`, …) the run can be switched into. */
data class ConfigMode(val id: String, val label: String, val description: String? = null)

/** One slash command the AGENT itself advertises (ACP available_commands). */
data class ConfigCommand(val name: String, val description: String, val hint: String? = null)

/** The whole live configuration: the chip vocabulary AND the values in force. */
data class SessionConfigState(
    val options: List<ConfigOption> = emptyList(),
    val currentMode: String? = null,
    val modes: List<ConfigMode> = emptyList(),
    val commands: List<ConfigCommand> = emptyList(),
)

/** The run's context window + spend as the engine last measured it. Token
 *  counts, deliberately NOT percentages — the device's rate-limit windows
 *  already own the 0-100 vocabulary and this is a different quantity. */
data class SessionUsageState(
    val contextUsed: Int,
    val contextSize: Int,
    val costUsd: Double? = null,
)

/** EXP-784: the agent's rate-limit window as it last reported it — the
 *  fourth latest-wins slot beside [SessionUsageState]. [status] is the
 *  agent's own word (`allowed_warning`, `rejected`, …); the slot is CLEARED
 *  by an empty/`ok` status ([rateLimitClears]), never filled by one. */
data class SessionRateLimitState(
    val status: String,
    /** Unix ms when the window resets, when the agent named one. */
    val resetsAt: Long? = null,
    val message: String? = null,
)

/** EXP-784: the `rate_limit.status` values that CLEAR the slot. */
fun rateLimitClears(status: String): Boolean =
    status.trim().let { it.isEmpty() || it.equals("ok", ignoreCase = true) }

/** EXP-818/831: whether a rate-limit report is a WALL worth a banner —
 *  `rejected`, or a notice the agent itself wrote. Claude files
 *  `allowed_warning` on every turn past ~75% of a window while it keeps
 *  working; the Usage sheet carries that percentage. Mirrors web
 *  `rateLimitIsWall` / desktop `steer::rate_limit_is_wall`. */
fun rateLimitIsWall(state: SessionRateLimitState): Boolean =
    state.status.trim() == "rejected" || !state.message.isNullOrBlank()

/** EXP-831: how long past its `resetsAt` a wall still renders (ms). The
 *  engine clears the slot on the run's next activity or its next rate-limit
 *  event; until one arrives (and on a journal replayed after the fact) the
 *  clock is the only thing that can drop a banner whose reset has come and
 *  gone. One minute covers clock skew. */
const val RATE_LIMIT_EXPIRY_GRACE_MS = 60_000L

/** EXP-831: whether the wall's reset is more than the grace behind [nowMs].
 *  A wall with no reset time never expires by the clock. Mirrored ×4. */
fun rateLimitExpired(state: SessionRateLimitState, nowMs: Long): Boolean =
    state.resetsAt?.let { nowMs - it > RATE_LIMIT_EXPIRY_GRACE_MS } ?: false

/** EXP-831: the banner's ONE gate — a wall whose reset has not passed. The
 *  screen re-reads it on its 30s activity clock. */
fun rateLimitBannerShows(state: SessionRateLimitState, nowMs: Long): Boolean =
    rateLimitIsWall(state) && !rateLimitExpired(state, nowMs)

/** EXP-785: a wire `toolKind`, or null for anything this build does not know. */
fun parseToolKind(raw: String?): String? = raw?.takeIf { it in DomainContract.toolKindValues }

/** EXP-850 (S1): claude's `TaskOutput` / `Monitor` calls — the agent is
 *  WAITING on something it started. An unsettled one is listed in the strip
 *  above the composer; in the transcript it stays an ordinary tool row, and
 *  `toolGroupSummary` counts it exactly like `other`. */
const val TOOL_KIND_WAIT = "wait"

/** EXP-850 (S1): the open waits, in feed order — an unsettled `wait` tool row
 *  labeled by its detail (the task's description), falling back to the tool's
 *  own name when the publisher resolved none. */
fun openWaitLabels(feed: List<AgentFeedItem>): List<String> = feed
    .filterIsInstance<AgentFeedItem.Tool>()
    .filter { it.toolKind == TOOL_KIND_WAIT && !it.settled }
    .map { it.detail?.takeIf { d -> d.isNotBlank() } ?: it.name }

/**
 * EXP-846: what an EXPONENTIAL MCP tool call returned, as the engine read it
 * out of the tool's JSON result and published on the call's `tool_update`
 * (`preview`). Every field is optional — the engine fills what the row it
 * touched actually carries, and the strings arrive already capped at 200 chars.
 * Phase 1 PLUMBS it only: the reducer keeps it on the tool row, the custom
 * issue-pill / PR-link / "N results" rendering is a later phase.
 */
data class ToolResultPreview(
    val id: String? = null,
    val identifier: String? = null,
    val title: String? = null,
    val url: String? = null,
    val count: Int? = null,
    val status: String? = null,
) {
    /** What this preview adds to a tool row's [feedItemBytes] estimate. */
    fun weight(): Int = (id?.length ?: 0) + (identifier?.length ?: 0) +
        (title?.length ?: 0) + (url?.length ?: 0) + (status?.length ?: 0)
}

// ── EXP-848: the agent's turn, as a latest-wins slot ────────────────────────
//
// The engine publishes ONE `turn` event per edge (`{kind:'turn',state}`, the
// contract's `turnState` values). Like `config_state`/`usage`/`rate_limit` it
// is STATE, never a feed row: the relay keeps the newest per room and replays
// it, so a late joiner learns whether the agent is mid-turn from one frame.

/** The agent is executing a turn right now. */
const val TURN_STATE_STARTED = "started"

/** The turn is over — `end_turn`, a cancel, or a prompt error. The DEFAULT
 *  everywhere: before any `turn` event arrives a client assumes idle, so
 *  nothing ever pulses just because a socket is up. */
const val TURN_STATE_ENDED = "ended"

/**
 * EXP-848: the ONE "the agent is working RIGHT NOW" rule — the busy footer, the
 * composer's Stop glyph and every other live-work cue read it and nothing
 * re-derives it. Byte-identical ×4 (web `agentWorking`, iOS, desktop
 * `steer::agent_working`): a live, unended run mid-turn with nothing parked on
 * it. Joined the socket but no turn yet ⇒ NOT working — [TURN_STATE_ENDED] is
 * the default, which is what stopped a freshly attached viewer from pulsing
 * over an idle agent.
 */
fun agentWorking(
    live: Boolean,
    sessionEnded: Boolean,
    turnState: String,
    awaitingInput: Boolean,
    needsInput: Boolean,
    blocked: Boolean,
    compacting: Boolean,
): Boolean = live && !sessionEnded && turnState == TURN_STATE_STARTED &&
    !awaitingInput && !needsInput && !blocked && !compacting

/** A chip whose value is blank — the CLI's own default. Byte-identical ×4. */
const val CONFIG_DEFAULT_VALUE_LABEL = "CLI default"

/** The mode chip's leading label — the only chip label left, so the only one
 *  the four clients have to agree on by hand. Byte-identical ×4. */
const val CONFIG_MODE_LABEL = "Mode"

/** EXP-772: the contract id of plan mode, and the label of the compact switch
 *  that replaces a `plan` + one other pair. Byte-identical ×4. */
const val PLAN_MODE_ID = "plan"
const val PLAN_TOGGLE_LABEL = "Plan"

/**
 * EXP-772: the composer's ONE steering control — the agent's mode. Model,
 * effort and every other option picker is gone from a running session: those
 * are launch decisions, and a mid-run swap only ever muddied the transcript.
 * The engine publishes an empty `config_state.options` to match.
 */
data class ModeChip(
    /** The mode in force, as the publisher labels it. */
    val valueLabel: String,
    /** Every advertised mode, in publisher order. */
    val values: List<ConfigValue> = emptyList(),
    /** Set when the run advertises exactly `plan` plus one other mode — the
     *  pair claude offers. The chip then draws as a compact "Plan" switch
     *  rather than a two-entry dropdown, because that is the only thing such a
     *  dropdown could ever say. */
    val planToggle: PlanToggle? = null,
) {
    /** The plan switch's two sides. */
    data class PlanToggle(
        /** The run is in plan mode right now. */
        val on: Boolean,
        val planId: String,
        /** Where flipping the switch OFF goes. */
        val otherId: String,
    )

    /** A chip with nothing to pick from is a read-only badge. */
    val readOnly: Boolean get() = values.isEmpty()
}

/**
 * The composer's one chip, or null when the run advertises no modes (codex) —
 * that draws nothing at all rather than an inert badge. Exactly `plan` + one
 * other mode collapses into the compact Plan switch; anything else stays a
 * picker over every advertised mode. Byte-identical ×4 (web
 * `steer-commands.ts`, iOS `AgentFeed.modeChip`, desktop `feed.rs`).
 */
fun modeChip(config: SessionConfigState?): ModeChip? {
    if (config == null || config.modes.isEmpty()) return null
    val current = config.modes.firstOrNull { it.id == config.currentMode }
    val plan = config.modes.firstOrNull { it.id == PLAN_MODE_ID }
    val other = config.modes.firstOrNull { it.id != PLAN_MODE_ID }
    return ModeChip(
        // An unadvertised current mode still reads as itself rather than as
        // "CLI default" — the agent is genuinely in it.
        valueLabel = current?.label
            ?: config.currentMode?.takeIf { it.isNotBlank() }
            ?: CONFIG_DEFAULT_VALUE_LABEL,
        values = config.modes.map { ConfigValue(it.id, it.label) },
        planToggle = if (config.modes.size == 2 && plan != null && other != null) {
            ModeChip.PlanToggle(
                on = config.currentMode == plan.id,
                planId = plan.id,
                otherId = other.id,
            )
        } else {
            null
        },
    )
}

/**
 * EXP-847: the session header's plan badge — the label of the mode the agent
 * is in right now, and ONLY while that is plan mode (`Plan` on every agent
 * that advertises one). Null otherwise, so an approved `ExitPlanMode` visibly
 * CLEARS the badge on the next `config_state` instead of leaving the run
 * looking like it is still planning.
 *
 * It is a READ-ONLY badge and nothing more (EXP-790: plan/model/effort are
 * launch-time, `config_state.options` is always empty) — which is what
 * [modeChip] is for here: the chip derivation stays the ONE place the current
 * mode's label is resolved, byte-identical ×4, and the header just renders its
 * `valueLabel`.
 */
fun planModeBadge(config: SessionConfigState?): String? {
    if (config?.currentMode != PLAN_MODE_ID) return null
    return modeChip(config)?.valueLabel?.takeIf { it.isNotBlank() }
}

/** Append a question card, or REPLACE the card carrying the same wire id in
 *  place (EXP-249) — a re-emission augments an ask (options the desktop
 *  discovers later) and must never stack a second card. The local feed id and
 *  any resolution already folded in are preserved. */
fun upsertQuestion(feed: List<AgentFeedItem>, item: AgentFeedItem.Question): List<AgentFeedItem> {
    val index = feed.indexOfFirst { it is AgentFeedItem.Question && it.wireId == item.wireId }
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

/**
 * EXP-772: fold a narration chunk into the newest row of its own lane when both
 * came out of the SAME assistant message.
 *
 * The engine's coalescer flushes one message in several `narration` events,
 * which used to draw one bubble per flush and shred a paragraph into a column
 * of fragments. Every event now carries the ACP [messageId] of its message, so
 * a flush whose message is the newest one in its lane APPENDS to that row (raw
 * concatenation — the flushes are chunks of one string, not sentences).
 * Anything of its OWN lane in between (a tool call, a human turn, a question)
 * ends the run: the message really did resume after something happened.
 *
 * EXP-846: the look-back skips OTHER lanes. A subagent's edges and its
 * scoped tool rows/updates interleave into the flat feed between two fragments
 * of one main-lane message, and they render inside that subagent's group, not
 * between the fragments — so matching only the row immediately behind shredded
 * every message a subagent ran underneath into fragments. [subagentId] null is
 * the main lane; the rule is symmetric (a subagent's own fragments merge across
 * main-lane rows, which sit outside its group too).
 *
 * Null = nothing to merge into, and the caller appends a fresh row.
 * Mirrored x4 (web `agent-feed.ts`, iOS `AgentFeed.mergeNarration`, desktop
 * `feed.rs`).
 */
fun mergeNarration(
    feed: List<AgentFeedItem>,
    text: String,
    messageId: String?,
    subagentId: String?,
): List<AgentFeedItem>? {
    if (messageId.isNullOrBlank()) return null
    for (index in feed.indices.reversed()) {
        val row = feed[index]
        // A row of another lane is not between these fragments as far as the
        // reader is concerned — step over it and keep looking back.
        if (row.feedLane() != subagentId) continue
        val narration = row as? AgentFeedItem.Narration ?: return null
        if (narration.messageId != messageId) return null
        return feed.toMutableList().apply {
            this[index] = narration.copy(text = narration.text + text)
        }
    }
    return null
}

/** Which lane a row renders in: a subagent's group, or the main feed (null).
 *  Unlike [subagentKey] this counts the subagent's own lifecycle edges, which
 *  ARE its group's header and footer. */
private fun AgentFeedItem.feedLane(): String? =
    if (this is AgentFeedItem.Subagent) subagentId else subagentKey()

/** Fold a `question_resolved` event into the feed (EXP-249): retire the card
 *  named by [id], else every card of [askId] — whose [answers] map onto the
 *  ask's steps in step order — else, with NEITHER given, every card still
 *  unresolved, [answers] landing positionally on the answer-consuming ones
 *  (the codex publisher's rollout cards carry no ids at all). A dismissal
 *  carries no answers. Null when nothing matched. */
fun resolveQuestions(
    feed: List<AgentFeedItem>,
    id: String?,
    askId: String?,
    answers: List<String> = emptyList(),
    dismissed: Boolean = false,
): List<AgentFeedItem>? {
    // EXP-820: a dismissal names the step that was showing AND its ask — the
    // whole ask is over (the engine cancelled it), so every step retires, or
    // the stepper would surface the next step of an ask nobody can answer and
    // keep the composer hidden behind it.
    if (id != null && !(dismissed && askId != null)) {
        val index = feed.indexOfFirst { it is AgentFeedItem.Question && it.wireId == id }
        if (index < 0) return null
        val item = feed[index] as AgentFeedItem.Question
        return feed.toMutableList().apply {
            // EXP-820: an ALREADY-resolved card takes a new answer too — a
            // re-answered step of an open ask resolves again with it.
            this[index] = item.copy(
                resolved = true,
                dismissed = item.dismissed || dismissed,
                answer = if (dismissed) item.answer else answers.firstOrNull() ?: item.answer,
            )
        }
    }
    if (askId == null) {
        if (feed.none { it is AgentFeedItem.Question && !it.resolved }) return null
        // Answers arrive in card order, so a cursor over the feed keeps them
        // aligned; the ask's final submit step consumes none.
        var cursor = 0
        return feed.map { item ->
            if (item !is AgentFeedItem.Question || item.resolved) {
                item
            } else {
                val answer = if (dismissed || item.isSubmitStep) {
                    item.answer
                } else {
                    answers.getOrNull(cursor)?.also { cursor += 1 } ?: item.answer
                }
                item.copy(resolved = true, dismissed = item.dismissed || dismissed, answer = answer)
            }
        }
    }
    val steps = orderedSteps(feed.filterIsInstance<AgentFeedItem.Question>().filter { it.askId == askId })
    if (steps.isEmpty()) return null
    val answerByFeedId = steps.withIndex().associate { (i, step) -> step.id to answers.getOrNull(i) }
    return feed.map { item ->
        if (item is AgentFeedItem.Question && item.askId == askId) {
            item.copy(
                resolved = true,
                dismissed = item.dismissed || dismissed,
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
    toolCalls: Int? = null,
    title: String? = null,
    /** EXP-850 (S4): a completed edge may be the first one naming the
     *  workflow; one that names none never erases it. */
    workflowId: String? = null,
): List<AgentFeedItem>? {
    val index = feed.indexOfLast {
        it is AgentFeedItem.Subagent && it.subagentId == subagentId && !it.completed
    }
    if (index < 0) return null
    val item = feed[index] as AgentFeedItem.Subagent
    return feed.toMutableList().apply {
        this[index] = item.copy(
            completed = true,
            detail = detail ?: item.detail,
            // EXP-748: the count only ever rides the completed edge, so the
            // fold is where it lands on the row.
            toolCalls = toolCalls ?: item.toolCalls,
            // EXP-847: a completed edge may be the first frame carrying the
            // description; one that carries none never erases it.
            title = title ?: item.title,
            workflowId = workflowId ?: item.workflowId,
        )
    }
}

/**
 * Ids of the [AgentFeedItem.Question] items still answerable: every card that
 * carries a wire id (EXP-249) and has not been retired by its own
 * `question_resolved`. The desktop's structured stream states question
 * lifetime outright, so no position guessing applies.
 *
 * Mirrors iOS `AgentFeed.activeQuestionIds` and web `activeQuestionIds`.
 */
fun activeQuestionIds(feed: List<AgentFeedItem>): Set<Long> {
    val ids = mutableSetOf<Long>()
    for (item in feed) {
        if (item is AgentFeedItem.Question && !item.resolved) ids.add(item.id)
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
        /** EXP-773: everything published under this subagent, in feed order —
         *  its tool calls plus the prose it wrote and the turns addressed to
         *  it, so the run reads as a conversation. The lifecycle markers stay
         *  out: they ARE the row. */
        val items: List<AgentFeedItem>,
        /** EXP-748: what the "N tool calls" caption counts — the visible tool
         *  rows or, when the publisher reported more (replay evicted a
         *  subagent's tool events), its count. 0 = nothing to say. */
        val toolCount: Int = 0,
        /** EXP-847: the spawning Agent call's description, from the first
         *  marker that carried one. Null = no publisher said. */
        val title: String? = null,
        /** EXP-850 (S4): the workflow this agent belongs to. Such a run is
         *  never a steerable tab and never a loose transcript row — it renders
         *  inside its workflow card. */
        val workflowId: String? = null,
    ) : AgentFeedRow
}

/** EXP-847: how a subagent run READS — its description when the spawning call
 *  gave one, else its agent type. ONE copy per client (the chips, the group row
 *  and the tab strip all call this). */
val AgentFeedRow.SubagentRun.label: String
    get() = title?.takeIf { it.isNotBlank() } ?: agentType

// ── EXP-787: the transcript's rhythm ────────────────────────────────────────
//
// Every row gets space ABOVE it, chosen from the row BEFORE it — ONE
// derivation, mirrored ×4 (web `lib/agent-feed.ts`, desktop `steer::feed`,
// ExpCore `AgentFeed`) and lock-tested on each. This file is compose-free, so
// it names the STEP and the screen maps it to `DesignTokens.Transcript.Gap*`.

/** What a row counts as when spacing it: a human turn, agent prose, or the
 *  machinery around them (tool calls, subagent runs, permission prompts). */
enum class AgentRowClass { Turn, Prose, Tool }

/** One step of the gap ladder — `DesignTokens.Transcript` holds the dp. */
enum class TranscriptGap { Turn, Block, Tool, Default, None }

/** Which class a rendered row belongs to. */
val AgentFeedRow.rowClass: AgentRowClass
    get() = when (this) {
        // A run of tool calls, and a subagent's whole run, are machinery.
        is AgentFeedRow.ToolRun -> AgentRowClass.Tool
        is AgentFeedRow.SubagentRun -> AgentRowClass.Tool
        is AgentFeedRow.QuestionStepper -> AgentRowClass.Prose
        is AgentFeedRow.Single -> when (item) {
            // Both shapes a human turn takes — the prose bubble and the
            // slash-command pill — are the same beat.
            is AgentFeedItem.UserMessage -> AgentRowClass.Turn
            is AgentFeedItem.Narration,
            is AgentFeedItem.Question,
            is AgentFeedItem.Compaction,
            -> AgentRowClass.Prose
            is AgentFeedItem.Tool,
            is AgentFeedItem.Subagent,
            is AgentFeedItem.DuplicateAgent,
            is AgentFeedItem.Permission,
            -> AgentRowClass.Tool
        }
    }

/**
 * The space above [cur] given the row before it — a null [prev] is the first
 * row, which gets none. In order: a turn on either side breathes widest, two
 * tool rows sit tightest, prose meeting a tool row takes the middle step, and
 * two prose rows take a paragraph's worth.
 */
fun transcriptGap(prev: AgentRowClass?, cur: AgentRowClass): TranscriptGap = when {
    prev == null -> TranscriptGap.None
    prev == AgentRowClass.Turn || cur == AgentRowClass.Turn -> TranscriptGap.Turn
    prev == AgentRowClass.Tool && cur == AgentRowClass.Tool -> TranscriptGap.Default
    prev == AgentRowClass.Tool || cur == AgentRowClass.Tool -> TranscriptGap.Tool
    else -> TranscriptGap.Block
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
fun groupFeedRows(
    feed: List<AgentFeedItem>,
    from: Int = 0,
    /** EXP-850 (S3/S4): the workflow cards this screen HAS — their agents and
     *  their duplicate warnings render inside the card instead of as loose
     *  transcript rows. Empty (the default) leaves every row where it is, so a
     *  tagged edge whose card never arrived is never lost. */
    workflowIds: Set<String> = emptySet(),
): List<AgentFeedRow> =
    pendingQuestionsLast(nestWorkflowRows(projectFeedRows(feed, from), workflowIds))

/** The projection WITHOUT the EXP-850 workflow nesting and without the S9
 *  reorder — every subagent run is a row of its own here, which is what makes
 *  [collectSubagents] see the workflow's agents too. */
private fun projectFeedRows(feed: List<AgentFeedItem>, from: Int = 0): List<AgentFeedRow> {
    // EXP-783: `from` restricts the projection to the rendered WINDOW. The
    // grouping starts from an empty state there, so a window that cuts through
    // a tool run, an ask or a subagent's calls opens a FRESH group at the
    // boundary — keyed on the first item the reader can actually see.
    // `from = 0` is the whole projection, item for item.
    @Suppress("NAME_SHADOWING")
    val feed = if (from <= 0) feed else feed.subList(minOf(from, feed.size), feed.size)
    val stepsByAsk = feed.filterIsInstance<AgentFeedItem.Question>()
        .filter { it.askId != null }
        .groupBy { it.askId!! }
    val markersBySubagent = feed.filterIsInstance<AgentFeedItem.Subagent>()
        .groupBy { it.subagentId }
    // EXP-773: prose and human turns are scoped too, so a subagent's whole
    // conversation groups under its row instead of interleaving into main.
    val itemsBySubagent = feed.filter { it !is AgentFeedItem.Subagent }
        .mapNotNull { item -> item.subagentKey()?.let { it to item } }
        .groupBy({ it.first }, { it.second })
    val emittedAsks = mutableSetOf<String>()
    val emittedSubagents = mutableSetOf<String>()
    val rows = mutableListOf<AgentFeedRow>()
    var i = 0
    while (i < feed.size) {
        val item = feed[i]
        val subagentId = item.feedLane()
        when {
            subagentId != null -> {
                if (emittedSubagents.add(subagentId)) {
                    val markers = markersBySubagent[subagentId].orEmpty()
                    val scoped = itemsBySubagent[subagentId].orEmpty()
                    val types = markers.map { it.agentType }.filter { it.isNotBlank() }
                    rows.add(
                        AgentFeedRow.SubagentRun(
                            id = (markers.map { it.id } + scoped.map { it.id }).min(),
                            subagentId = subagentId,
                            agentType = types.firstOrNull { it != SUBAGENT_FALLBACK_TYPE }
                                ?: types.firstOrNull()
                                ?: SUBAGENT_FALLBACK_TYPE,
                            completed = markers.any { it.completed },
                            detail = markers.lastOrNull { it.detail != null }?.detail,
                            // EXP-847: the first description any marker of this
                            // run carried — the started edge normally, the
                            // completed one when that is all we kept.
                            title = markers.firstNotNullOfOrNull {
                                it.title?.takeIf { t -> t.isNotBlank() }
                            },
                            items = scoped,
                            toolCount = maxOf(
                                scoped.count { it is AgentFeedItem.Tool },
                                markers.mapNotNull { it.toolCalls }.maxOrNull() ?: 0,
                            ),
                            // EXP-850 (S4): the first marker that named a
                            // workflow — this run belongs to that card.
                            workflowId = markers.firstNotNullOfOrNull { it.workflowId },
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

/**
 * EXP-850 (S3): the `Workflow` tool row renders as its CARD, so it can never
 * sit collapsed inside a "N tool calls" group — a run of consecutive tool rows
 * is split around every call whose id has a card. The surrounding rows stay
 * grouped (a lone survivor becomes a Single, exactly as if it had never had a
 * neighbour), and the row ids stay the first covered item's, so nothing the
 * reader is anchored to moves.
 */
fun splitWorkflowToolRows(
    rows: List<AgentFeedRow>,
    workflowIds: Set<String>,
): List<AgentFeedRow> {
    if (workflowIds.isEmpty()) return rows
    if (rows.none { it is AgentFeedRow.ToolRun && it.items.any { i -> i.callId in workflowIds } }) {
        return rows
    }
    val out = mutableListOf<AgentFeedRow>()
    for (row in rows) {
        if (row !is AgentFeedRow.ToolRun || row.items.none { it.callId in workflowIds }) {
            out.add(row)
            continue
        }
        val run = mutableListOf<AgentFeedItem.Tool>()
        fun flush() {
            when (run.size) {
                0 -> Unit
                1 -> out.add(AgentFeedRow.Single(run.first()))
                else -> out.add(AgentFeedRow.ToolRun(run.toList()))
            }
            run.clear()
        }
        for (item in row.items) {
            if (item.callId in workflowIds) {
                flush()
                out.add(AgentFeedRow.Single(item))
            } else {
                run.add(item)
            }
        }
        flush()
    }
    return out
}

/** Ask steps in stepper order: by 1-based index, with the index-less final
 *  submit step last. */
fun orderedSteps(steps: List<AgentFeedItem.Question>): List<AgentFeedItem.Question> =
    steps.sortedWith(compareBy(nullsLast<Int>()) { it.index })

/** Every subagent seen in the feed, in first-appearance order (EXP-356) — the
 *  session screen renders one conversation tab per run, labeled and summarized
 *  exactly like its group row (iOS/web parity). */
fun collectSubagents(feed: List<AgentFeedItem>): List<AgentFeedRow.SubagentRun> =
    projectFeedRows(feed).filterIsInstance<AgentFeedRow.SubagentRun>()

/**
 * EXP-850 (S3/S4): a workflow's own rows leave the main transcript — its
 * agents' runs and the duplicate warnings tagged with its id render INSIDE its
 * card.
 *
 * Only for cards this screen actually HOLDS ([workflowIds]): an edge tagged
 * with a workflow whose frame never arrived keeps its ordinary row, because a
 * warning nobody can see is the one thing EXP-856 exists to prevent.
 */
private fun nestWorkflowRows(
    rows: List<AgentFeedRow>,
    workflowIds: Set<String>,
): List<AgentFeedRow> {
    if (workflowIds.isEmpty()) return rows
    return rows.filterNot { row ->
        when (row) {
            is AgentFeedRow.SubagentRun -> row.workflowId in workflowIds
            is AgentFeedRow.Single ->
                (row.item as? AgentFeedItem.DuplicateAgent)?.workflowId in workflowIds
            else -> false
        }
    }
}

/** EXP-850 (S3/S4): the workflow's own rows, for the card to render — its
 *  agents' runs (in feed order) and the duplicate warnings it collected. */
fun workflowAgentRuns(
    feed: List<AgentFeedItem>,
    workflowId: String,
): List<AgentFeedRow.SubagentRun> =
    collectSubagents(feed).filter { it.workflowId == workflowId }

/** EXP-856: the duplicate warnings belonging to [workflowId] — rendered under
 *  its card, and kept there even when the card is collapsed. */
fun workflowDuplicates(
    feed: List<AgentFeedItem>,
    workflowId: String,
): List<AgentFeedItem.DuplicateAgent> =
    feed.filterIsInstance<AgentFeedItem.DuplicateAgent>().filter { it.workflowId == workflowId }

/**
 * EXP-850 (S9): a question or plan still waiting on this client moves AFTER
 * every later row — the card the agent is blocked on belongs at the bottom of
 * the transcript, where the composer would be, not buried under the tool rows
 * that kept arriving behind it. Pending rows keep their relative order, and a
 * resolved one drops straight back into its natural place.
 */
fun pendingQuestionsLast(rows: List<AgentFeedRow>): List<AgentFeedRow> {
    val pending = rows.filter { it.awaitsAnswer }
    if (pending.isEmpty() || pending.size == rows.size) return rows
    // Already last, in order: nothing to move.
    if (rows.takeLast(pending.size) == pending) return rows
    return rows.filterNot { it.awaitsAnswer } + pending
}

/** Whether a rendered row is a question this client could still answer — a
 *  single card or any step of an ask, unresolved and not dismissed. */
val AgentFeedRow.awaitsAnswer: Boolean
    get() = when (this) {
        is AgentFeedRow.Single ->
            (item as? AgentFeedItem.Question)?.let { !it.resolved && !it.dismissed } == true
        is AgentFeedRow.QuestionStepper -> steps.any { !it.resolved && !it.dismissed }
        else -> false
    }

/** The tabs the strip actually shows (EXP-387): running subagents, plus the
 *  focused one even when done — a completion never yanks the user out of a
 *  conversation they are reading; the tab disappears once they click away.
 *  Completed runs stay readable via their inline group row in Main. */
fun visibleSubagentTabs(
    agents: List<AgentFeedRow.SubagentRun>,
    selected: String?,
): List<AgentFeedRow.SubagentRun> =
    // EXP-850 (S3): a workflow's agents are never tabs and never steerable —
    // they are read inside the workflow card that owns them.
    agents.filter { it.workflowId == null && (!it.completed || it.subagentId == selected) }

/** The step a stepper card should show: the first one still waiting on this
 *  client, or null once every step is answered — the card then renders the
 *  whole ask with its answers. [answered] holds the wire ids of steps whose
 *  answer is out (sent or acknowledged — a dropped Sending lock re-surfaces
 *  its step). */
fun currentStepperStep(
    steps: List<AgentFeedItem.Question>,
    answered: Set<String>,
): AgentFeedItem.Question? =
    steps.firstOrNull { step -> !step.resolved && step.wireId !in answered }

/**
 * EXP-820: whether an ask is OVER — nothing waits on the agent and no step can
 * be re-answered. The ONE rule, byte-identical on web, the IDE and iOS:
 * - its submit step (`askId` set, no `index`) exists and has resolved, or
 * - it has at most one step and every numbered step has resolved, or
 * - any step was dismissed (the engine cancelled the ask).
 * A multi-step ask whose numbered steps have all resolved but whose submit
 * step has not is still OPEN: the engine re-records an earlier step's
 * `answer` frame until the submit lands, which is what "back and forth" in the
 * stepper rides on.
 */
fun askComplete(steps: List<AgentFeedItem.Question>): Boolean {
    if (steps.any { it.dismissed }) return true
    if (steps.any { it.isSubmitStep && it.resolved }) return true
    val numbered = steps.filter { !it.isSubmitStep }
    val total = numbered.firstOrNull()?.total ?: numbered.size
    return total <= 1 && numbered.isNotEmpty() && numbered.all { it.resolved }
}

/** EXP-820 (byte-identical ×4): the inline field a "Type something." row opens. */
const val FREE_TEXT_ANSWER_PLACEHOLDER = "Type your answer…"

/** EXP-820 (byte-identical ×4): the inline field a plan's reject row opens. */
const val PLAN_FEEDBACK_PLACEHOLDER = "Tell the agent what to change…"

/** EXP-820 (byte-identical ×4): leaves an earlier step re-opened for editing. */
const val BACK_TO_CURRENT_STEP_LABEL = "Back to current step"

// Wire-field readers: a field of an unexpected shape reads as absent, never
// throws — one malformed event must not tear down the socket.
private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull

private fun JsonObject.bool(key: String): Boolean =
    (this[key] as? JsonPrimitive)?.booleanOrNull == true

private fun JsonObject.int(key: String): Int? = (this[key] as? JsonPrimitive)?.intOrNull
private fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.longOrNull
private fun JsonObject.dbl(key: String): Double? = (this[key] as? JsonPrimitive)?.doubleOrNull

/** Everything the room's activity log owns, as ONE immutable value — the whole
 *  decode path is a pure transition over it, so it is unit testable end to end
 *  and the ViewModel is left publishing the result. */
/** EXP-724: the compaction in flight behind the pinned [COMPACTING_LABEL]
 *  strip. [trigger] is the publisher's `manual`/`auto` when it knows. */
data class CompactionState(
    val trigger: String? = null,
    /** The frame's own `at` stamp (ms) when it carried one — a replayed
     *  `started` from long ago has already used its backstop budget. */
    val startedAtMs: Long? = null,
)

data class ActivityFeedState(
    val feed: List<AgentFeedItem> = emptyList(),
    /** The most recent worktree diff — each one replaces the previous. */
    val latestDiff: String? = null,
    /** EXP-724: set by `compaction started`, cleared by `ended`, by the replay
     *  swap (which re-folds from a fresh state) and by the connection's
     *  [COMPACTION_TIMEOUT_MS] backstop. Never a feed row. */
    val compacting: CompactionState? = null,
    /** EXP-746: the agent's live configuration behind the composer chips.
     *  Latest-wins state beside the feed, like [compacting] — never a row. */
    val config: SessionConfigState? = null,
    /** EXP-746: this run's context + spend meter, same latest-wins rule. */
    val usage: SessionUsageState? = null,
    /** EXP-784: the agent's rate-limit window, the fourth slot. Null = not
     *  limited (or cleared by an empty/`ok` status). */
    val rateLimit: SessionRateLimitState? = null,
    /** EXP-848: the agent's turn edge, the fifth slot — [TURN_STATE_STARTED]
     *  or [TURN_STATE_ENDED], and ENDED until a `turn` event says otherwise
     *  (an older publisher sends none, and an idle run must never pulse). */
    val turnState: String = TURN_STATE_ENDED,
    /** EXP-850 (S5): when the running turn started (unix ms), as the engine
     *  stamped it — the working caption's clock AND the seed of its verb, so
     *  the wording is stable for the whole turn. Null before any publisher
     *  said (a codex run, an older engine): the caption then has no suffix. */
    val turnStartedAt: Long? = null,
    /** EXP-850 (S5): output tokens produced in the running turn so far,
     *  republished at most every `steerWorking.tokenTickMs`. A republish that
     *  omits it never blanks what we already learned; a NEW [turnStartedAt]
     *  resets it, because those tokens belong to the turn that is over. */
    val turnTokens: Long? = null,
    /** EXP-850 (S2): what the machine is running in the background, as ONE
     *  latest-wins list — an empty one closes the strip above the composer. */
    val backgroundTasks: List<BackgroundTask> = emptyList(),
    /** EXP-850 (S3): one card per workflow, latest-wins PER ID, in
     *  first-appearance order and capped at [WORKFLOW_SLOT_CAP] (oldest id
     *  evicted). Never feed rows: a card is patched onto the `tool` row
     *  carrying the same id. */
    val workflows: List<WorkflowState> = emptyList(),
    /** Per-card answer locks, keyed by the card's wire id (EXP-249). */
    val answerLocks: Map<String, AnswerState> = emptyMap(),
    /** What THIS client picked per locked card — the option labels (a typed
     *  free-text reply in place of its row's label). The desktop only fills a
     *  question's `answer` on `question_resolved`, which for a multi-question
     *  ask lands after the WHOLE ask submits, so answered stepper steps would
     *  otherwise read "Answered" ×N until then (EXP-588, web/iOS parity).
     *  Dropped with a failed lock — a rolled-back step has no answer. */
    val answerLabels: Map<String, List<String>> = emptyMap(),
    val nextEventId: Long = 0L,
    /** EXP-783: the running weight of [feed] against [FEED_BYTE_CAP], so the
     *  budget is an integer compare per append rather than a walk. */
    val feedBytes: Long = 0L,
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
    /** EXP-783: the publisher's wire sequence for this event, stamped onto
     *  whatever row it produces. Absent from a publisher older than EXP-783. */
    seq: Long? = null,
): ActivityFeedState = when (event.str("kind")) {
    "narration" -> {
        val text = event.str("text").orEmpty()
        if (text.isBlank()) {
            this
        } else {
            val messageId = event.str("messageId")?.takeIf { it.isNotBlank() }
            val subagentId = event.str("subagentId")?.takeIf { it.isNotBlank() }
            val row = AgentFeedItem.Narration(nextEventId, text, messageId, subagentId, seq)
            // EXP-483: prose from the withheld ask/plan entry flushes AFTER
            // its already-published card — splice it back above.
            val anchor = event.str("beforeQuestionId")?.takeIf { it.isNotBlank() }
            val spliced = anchor?.let { spliceBeforeQuestion(feed, it, row) }
            // EXP-772: consecutive flushes of ONE assistant message are one
            // bubble. A merge consumes no feed id — the row it grew has one.
            val merged = if (spliced == null) {
                mergeNarration(feed, text, messageId, subagentId)
            } else {
                null
            }
            when {
                spliced != null -> withFeed(spliced).copy(nextEventId = nextEventId + 1)
                // A merge grows an existing row's text rather than adding one.
                merged != null ->
                    copy(feed = merged, feedBytes = feedBytes + text.length).trimmed()
                else -> append(row)
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
                    seq = seq,
                    callId = event.str("id")?.takeIf { it.isNotBlank() },
                    toolKind = parseToolKind(event.str("toolKind")),
                ),
            )
        }
    }
    // EXP-785/786: folded INTO the newest tool row with that call id — a
    // settle, a per-call diff, or both. Never a row of its own; an id this
    // feed does not hold (evicted, below the window, a pre-EXP-785 row) is
    // dropped. A `failed` after a `completed` wins; a status-less update
    // carrying only a diff never settles the call.
    "tool_update" -> {
        val id = event.str("id")?.takeIf { it.isNotBlank() }
        val at = if (id == null) -1 else feed.indexOfLast { it is AgentFeedItem.Tool && it.callId == id }
        if (at < 0) {
            this
        } else {
            val row = feed[at] as AgentFeedItem.Tool
            val status = event.str("status")
            val settles = status == "completed" || status == "failed"
            val next = row.copy(
                settled = row.settled || settles,
                failed = if (settles) status == "failed" else row.failed,
                diff = event.str("diff")?.takeIf { it.isNotBlank() } ?: row.diff,
                // EXP-846: only Exponential MCP calls carry one; an update
                // without it leaves whatever the call already previewed.
                preview = toolPreview(event["preview"]) ?: row.preview,
            )
            withFeed(feed.toMutableList().also { it[at] = next })
        }
    }
    // Diffs never enter the feed — the latest replaces the previous one behind
    // the pinned "Latest changes" chip.
    "diff" -> copy(latestDiff = event.str("diff")?.takeIf { it.isNotBlank() })
    "user_message" -> {
        val text = event.str("text")
        if (text.isNullOrBlank() || isEcho(text)) {
            this
        } else {
            // EXP-773: a turn addressed to a subagent renders inside that
            // subagent's run, never in the main thread.
            append(
                AgentFeedItem.UserMessage(
                    id = nextEventId,
                    text = text,
                    subagentId = event.str("subagentId")?.takeIf { it.isNotBlank() },
                    seq = seq,
                ),
            )
        }
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
        // The wire id is REQUIRED: it addresses the `answer` frame and every
        // resolution event, so a card without one could never be answered nor
        // retired. No publisher emits one, and the relay rejects it.
        val wireId = event.str("id")?.takeIf { it.isNotBlank() }
        if (text.isNullOrBlank() || options.isEmpty() || wireId == null) {
            this
        } else {
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
                seq = seq,
            )
            val next = upsertQuestion(feed, question)
            withFeed(next).copy(
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
        (if (next != null) withFeed(next) else this).releaseResolvedLocks()
    }
    "answer_ack" -> {
        val id = event.str("id")?.takeIf { it.isNotBlank() }
        if (id == null) {
            this
        } else {
            // The desktop injected the answer: the card stays locked on EVERY
            // viewer (not just the sender) and a stepper advances a step.
            copy(answerLocks = answerLocks + (id to AnswerState.Acked))
        }
    }
    "subagent" -> {
        val subagentId = event.str("id")?.takeIf { it.isNotBlank() }
        val detail = event.str("detail")?.takeIf { it.isNotBlank() }
        val status = event.str("status")
        val completed = status == "completed"
        val toolCalls = event.int("toolCalls")?.takeIf { it >= 0 }
        // EXP-847: the spawning Agent call's own description of the run.
        val title = event.str("title")?.takeIf { it.isNotBlank() }
        // EXP-850 (S4): the workflow this agent belongs to, when it belongs to
        // one — its edges nest under that card and never become a tab.
        val workflowId = event.str("workflowId")?.takeIf { it.isNotBlank() }
        val closed = if (subagentId != null && completed) {
            completeSubagent(feed, subagentId, detail, toolCalls, title, workflowId)
        } else {
            null
        }
        when {
            subagentId == null -> this
            // EXP-856: a second copy of a still-running agent. Its own row,
            // never folded into the agent's lifecycle marker — the warning has
            // to survive the group collapsing. A publisher that sends the
            // status without the sentence is skipped rather than guessed at.
            status == "duplicate" -> if (detail == null) {
                this
            } else {
                append(
                    AgentFeedItem.DuplicateAgent(
                        id = nextEventId,
                        subagentId = subagentId,
                        detail = detail,
                        title = title,
                        workflowId = workflowId,
                        seq = seq,
                    ),
                )
            }
            closed != null -> withFeed(closed)
            else -> append(
                AgentFeedItem.Subagent(
                    id = nextEventId,
                    subagentId = subagentId,
                    agentType = event.str("agentType")?.takeIf { it.isNotBlank() } ?: "agent",
                    completed = completed,
                    detail = detail,
                    toolCalls = toolCalls,
                    title = title,
                    workflowId = workflowId,
                    seq = seq,
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
                    seq = seq,
                ),
            )
        }
    }
    // EXP-724: the strip is state beside the diff, never a row; the end leaves
    // a marker so "why did it forget everything" has an answer later. A bare
    // `ended` (codex auto-compaction publishes no start) still marks.
    "compaction" -> when (event.str("phase")) {
        "started" -> copy(
            compacting = CompactionState(
                trigger = event.str("trigger")?.takeIf { it.isNotBlank() },
                startedAtMs = event.long("at"),
            ),
        )
        "ended" -> copy(compacting = null).append(AgentFeedItem.Compaction(nextEventId, seq))
        else -> this
    }
    // EXP-746: the live configuration behind the composer chips. LATEST-WINS
    // STATE, never a row — one frame carries the whole picture, so a newer one
    // simply replaces the older. A payload we cannot read at all leaves the
    // PREVIOUS snapshot standing: blanking the chips on one odd frame would
    // read as the agent having lost its settings.
    "config_state" -> runCatching {
        val options = event["options"]!!.jsonArray.mapNotNull { raw ->
            val option = raw as? JsonObject ?: return@mapNotNull null
            val id = option.str("id")?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
            ConfigOption(
                id = id,
                label = option.str("label").orEmpty(),
                category = option.str("category")?.takeIf { it.isNotBlank() },
                // Kept as-is: a BLANK value is the "CLI default" choice, which
                // is not the same as an option that advertises no value.
                value = option.str("value"),
                values = configValues(option["values"]),
            )
        }
        copy(
            config = SessionConfigState(
                options = options,
                currentMode = event.str("currentMode")?.takeIf { it.isNotBlank() },
                modes = (event["modes"] as? JsonArray).orEmptyList { mode ->
                    val id = mode.str("id")?.takeIf { it.isNotBlank() } ?: return@orEmptyList null
                    ConfigMode(
                        id = id,
                        label = mode.str("label").orEmpty(),
                        description = mode.str("description")?.takeIf { it.isNotBlank() },
                    )
                },
                commands = (event["commands"] as? JsonArray).orEmptyList { command ->
                    val name = command.str("name")?.takeIf { it.isNotBlank() }
                        ?: return@orEmptyList null
                    ConfigCommand(
                        name = name,
                        description = command.str("description").orEmpty(),
                        hint = command.str("hint")?.takeIf { it.isNotBlank() },
                    )
                },
            ),
        )
    }.getOrDefault(this)
    // EXP-746: the context + spend meter, same latest-wins rule. A ZERO
    // context size means the engine does not know the window (claude before
    // its first result, an agent that reports none) — that CLEARS the slot
    // rather than rendering a "0 / 0 (0%)" bar; an unreadable payload keeps
    // the previous numbers.
    "usage" -> {
        val used = event.int("contextUsed")
        val size = event.int("contextSize")
        when {
            used == null || size == null || used < 0 || size < 0 -> this
            size == 0 -> copy(usage = null)
            else -> copy(
                usage = SessionUsageState(
                    contextUsed = used,
                    contextSize = size,
                    costUsd = event.dbl("costUsd")?.takeIf { it >= 0.0 },
                ),
            )
        }
    }
    // EXP-848: the fifth slot — the agent's turn edge. A state this build
    // cannot name leaves the slot standing (the `config_state` rule): guessing
    // would either pulse an idle run or still a working one.
    // EXP-850 (S5): the same slot now carries the turn's clock and its output
    // tokens. A republish that omits either NEVER blanks what we already
    // learned (the engine only restates what changed), but a NEW `startedAt`
    // is a new turn, and its token count starts from whatever that frame says.
    "turn" -> {
        val state = event.str("state")?.takeIf { it in DomainContract.turnStateValues }
        val startedAt = event.long("startedAt")?.takeIf { it > 0L }
        val tokens = event.long("tokens")?.takeIf { it >= 0L }
        val freshTurn = startedAt != null && startedAt != turnStartedAt
        copy(
            turnState = state ?: turnState,
            turnStartedAt = startedAt ?: turnStartedAt,
            turnTokens = when {
                freshTurn -> tokens
                tokens != null -> tokens
                else -> turnTokens
            },
        )
    }
    // EXP-850 (S2): the machine's background tasks, as ONE latest-wins list —
    // an EMPTY array is meaningful (nothing is running any more), so only a
    // payload that is not an array at all leaves the previous list standing.
    "background_tasks" -> {
        val raw = event["tasks"] as? JsonArray
        if (raw == null) {
            this
        } else {
            copy(
                backgroundTasks = raw.orEmptyList { task ->
                    val id = task.str("id")?.takeIf { it.isNotBlank() } ?: return@orEmptyList null
                    val description = task.str("description")?.takeIf { it.isNotBlank() }
                        ?: return@orEmptyList null
                    BackgroundTask(
                        id = id,
                        kind = task.str("kind")
                            ?.takeIf { it in DomainContract.backgroundTaskKindValues }
                            ?: BACKGROUND_TASK_KIND_OTHER,
                        description = description,
                        toolId = task.str("toolId")?.takeIf { it.isNotBlank() },
                    )
                },
            )
        }
    }
    // EXP-850 (S3): one card per workflow, latest-wins PER ID. Never a feed
    // row — the screen patches it onto the `tool` row carrying the same id.
    "workflow" -> {
        val id = event.str("id")?.takeIf { it.isNotBlank() }
        val name = event.str("name")?.takeIf { it.isNotBlank() }
        if (id == null || name == null) {
            this
        } else {
            val next = WorkflowState(
                id = id,
                name = name,
                description = event.str("description")?.takeIf { it.isNotBlank() },
                status = event.str("status")
                    ?.takeIf { it in DomainContract.workflowStatusValues }
                    ?: WORKFLOW_STATUS_RUNNING,
                phases = (event["phases"] as? JsonArray).orEmptyList { phase ->
                    val index = phase.int("index") ?: return@orEmptyList null
                    WorkflowPhase(index, phase.str("title").orEmpty())
                },
                agents = (event["agents"] as? JsonArray).orEmptyList { agent ->
                    val index = agent.int("index") ?: return@orEmptyList null
                    WorkflowAgent(
                        index = index,
                        label = agent.str("label").orEmpty(),
                        phaseIndex = agent.int("phaseIndex"),
                        agentId = agent.str("agentId")?.takeIf { it.isNotBlank() },
                        model = agent.str("model")?.takeIf { it.isNotBlank() },
                        state = agent.str("state")
                            ?.takeIf { it in DomainContract.workflowAgentStateValues }
                            ?: WORKFLOW_AGENT_STATE_QUEUED,
                        tokens = agent.long("tokens")?.takeIf { it >= 0L },
                        toolCalls = agent.int("toolCalls")?.takeIf { it >= 0 },
                        durationMs = agent.long("durationMs")?.takeIf { it >= 0L },
                        lastTool = agent.str("lastTool")?.takeIf { it.isNotBlank() },
                        lastToolSummary = agent.str("lastToolSummary")?.takeIf { it.isNotBlank() },
                        resultPreview = agent.str("resultPreview")?.takeIf { it.isNotBlank() },
                        error = agent.str("error")?.takeIf { it.isNotBlank() },
                    )
                },
                summary = event.str("summary")?.takeIf { it.isNotBlank() },
            )
            copy(workflows = upsertWorkflow(workflows, next))
        }
    }
    // EXP-784: the fourth slot. Null clears — an empty/`ok` status says the
    // window lifted, and an unreadable payload must not leave a stale "rate
    // limited" banner beside a live run.
    "rate_limit" -> {
        val status = event.str("status")
        copy(
            rateLimit = if (status == null || rateLimitClears(status)) {
                null
            } else {
                SessionRateLimitState(
                    status = status.trim(),
                    resetsAt = event.long("resetsAt")?.takeIf { it >= 0L },
                    message = event.str("message")?.trim()?.takeIf { it.isNotEmpty() },
                )
            },
        )
    }
    else -> this
}

/** EXP-850 (S3): latest-wins per id, keeping FIRST-APPEARANCE order — a card
 *  that updates must not jump to the end of the list — and capped at
 *  [WORKFLOW_SLOT_CAP] with the oldest id evicted, exactly like the journal
 *  and the relay's slot store. */
fun upsertWorkflow(workflows: List<WorkflowState>, next: WorkflowState): List<WorkflowState> {
    val at = workflows.indexOfFirst { it.id == next.id }
    if (at >= 0) {
        if (workflows[at] == next) return workflows
        return workflows.toMutableList().apply { this[at] = next }
    }
    val grown = workflows + next
    return if (grown.size <= WORKFLOW_SLOT_CAP) {
        grown
    } else {
        grown.subList(grown.size - WORKFLOW_SLOT_CAP, grown.size).toList()
    }
}

/** The card for the `tool` row carrying [id], when one has been published. */
fun ActivityFeedState.workflowFor(id: String?): WorkflowState? =
    id?.let { key -> workflows.firstOrNull { it.id == key } }

/** EXP-850 (S5): the NEWEST workflow still running — whose caption replaces
 *  the working row's verb, and which the device mirrors into the session
 *  row's `agent_caption`. Null when none runs. */
fun ActivityFeedState.runningWorkflow(): WorkflowState? = workflows.lastOrNull { it.isRunning }

/** EXP-846: one `tool_update.preview` object. Null when the field is absent or
 *  not an object, and an object with nothing readable in it is null too — a
 *  preview that says nothing must not mark the row as having one. */
private fun toolPreview(raw: JsonElement?): ToolResultPreview? {
    val obj = raw as? JsonObject ?: return null
    val preview = ToolResultPreview(
        id = obj.str("id")?.takeIf { it.isNotBlank() },
        identifier = obj.str("identifier")?.takeIf { it.isNotBlank() },
        title = obj.str("title")?.takeIf { it.isNotBlank() },
        url = obj.str("url")?.takeIf { it.isNotBlank() },
        count = obj.int("count")?.takeIf { it >= 0 },
        status = obj.str("status")?.takeIf { it.isNotBlank() },
    )
    return preview.takeIf { it != ToolResultPreview() }
}

/** One `values[]` array of a `config_state` option; anything unusable drops
 *  that entry, never the whole option (the `question.options` precedent). */
private fun configValues(raw: JsonElement?): List<ConfigValue> =
    (raw as? JsonArray).orEmptyList { value ->
        val id = value.str("id")?.takeIf { it.isNotBlank() } ?: return@orEmptyList null
        ConfigValue(id, value.str("label").orEmpty())
    }

/** Map a wire array, dropping entries that are not objects or that [decode]
 *  rejects; a null array is simply empty. */
private inline fun <T> JsonArray?.orEmptyList(decode: (JsonObject) -> T?): List<T> =
    this?.mapNotNull { entry -> (entry as? JsonObject)?.let(decode) } ?: emptyList()

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
        .map { it.wireId }
        .toSet()
    val next = answerLocks - done
    return if (next.size == answerLocks.size) this else copy(answerLocks = next)
}

/** Drop the compaction strip without an `ended` frame (EXP-724): the
 *  connection's [COMPACTION_TIMEOUT_MS] backstop, or the session ending under
 *  it. Leaves no marker — nothing was observed to finish. */
fun ActivityFeedState.clearCompaction(): ActivityFeedState =
    if (compacting == null) this else copy(compacting = null)

/** EXP-848: force the turn slot back to idle — the session ended under us, so
 *  whatever turn was in flight is not in flight any more and nothing may keep
 *  reading as working. */
fun ActivityFeedState.clearTurn(): ActivityFeedState =
    if (turnState == TURN_STATE_ENDED) this else copy(turnState = TURN_STATE_ENDED)

/** A locally-echoed steered message, shown before its transcript twin. */
fun ActivityFeedState.appendUserMessage(text: String): ActivityFeedState =
    append(AgentFeedItem.UserMessage(nextEventId, text))

private fun ActivityFeedState.append(item: AgentFeedItem): ActivityFeedState =
    copy(
        feed = feed + item,
        feedBytes = feedBytes + feedItemBytes(item),
        nextEventId = nextEventId + 1,
    ).trimmed()

/** The feed changed SHAPE (a splice, an in-place card replacement, a
 *  resolution): the running weight is cheaper to re-derive than to track. */
private fun ActivityFeedState.withFeed(next: List<AgentFeedItem>): ActivityFeedState =
    copy(feed = next, feedBytes = next.sumOf { feedItemBytes(it) }).trimmed()

/** EXP-783: evict from the OLDEST end only once the run exceeds
 *  [FEED_BYTE_CAP] or [FEED_ITEM_CAP], with the 90% hysteresis every client
 *  shares (`steer::feed::trim`): evicting down to the budget exactly would
 *  evict again on the very next event, and every eviction reallocates. The
 *  newest row always survives. */
fun ActivityFeedState.trimmed(): ActivityFeedState {
    if (feedBytes <= FEED_BYTE_CAP && feed.size <= FEED_ITEM_CAP) return this
    val percent = DomainContract.steerFeedTrimTargetPercent
    val byteTarget = FEED_BYTE_CAP * percent / 100
    val itemTarget = FEED_ITEM_CAP * percent / 100
    var remaining = feedBytes
    var dropTo = 0
    while (dropTo + 1 < feed.size && (remaining > byteTarget || feed.size - dropTo > itemTarget)) {
        remaining -= feedItemBytes(feed[dropTo])
        dropTo++
    }
    if (dropTo == 0) return this
    return copy(feed = feed.subList(dropTo, feed.size).toList(), feedBytes = maxOf(0, remaining))
}

/**
 * EXP-773: where an ENDED run's transcript is coming from. The relay has no
 * room for a finished session, so it asks the device that ran it to republish
 * its on-disk journal; these are the three answers.
 */
enum class HistoryState {
    /** The relay woke the device and is waiting for it to publish. */
    Pending,

    /** The machine holding the journal is not reachable. Terminal: nothing to
     *  redial for, the file is on that disk. */
    DeviceOffline,

    /** The device answered and has no journal for this run. Terminal. */
    Unavailable,
    ;

    /** Nothing is going to change on its own — stop dialing. */
    val terminal: Boolean get() = this != Pending
}

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

// ── EXP-786: the per-call diff ───────────────────────────────────────────────

/** [splitTruncatedDiff]'s two halves: the diff proper, and how many lines the
 *  publisher dropped (null when it dropped none). */
data class TruncatedDiff(val diff: String, val truncated: Int?)

/** A publisher-cut diff ends in ONE metadata line saying how much it dropped
 *  (`\ 120 more lines truncated`). Anchored with `\z`, never `$`, because
 *  Java's `$` also matches before a FINAL line terminator and JS's does not —
 *  the web regex (`lib/agent-feed.ts`) is the contract. */
private val DIFF_TRUNCATION_LINE = Regex("""(?:^|\n)\\ (\d+) more lines? truncated\s*\z""")

/** Split the trailing truncation note off: the diff proper renders as a diff,
 *  the note as a muted footer. Mirrors web `splitTruncatedDiff`. */
fun splitTruncatedDiff(diff: String): TruncatedDiff {
    val match = DIFF_TRUNCATION_LINE.find(diff) ?: return TruncatedDiff(diff, null)
    // A count too big for an Int is not a footer we can word — leave the diff
    // whole rather than dropping its last line for an unrenderable number.
    val truncated = match.groupValues[1].toIntOrNull() ?: return TruncatedDiff(diff, null)
    return TruncatedDiff(diff.substring(0, match.range.first), truncated)
}

/** The footer's words. Locked x4 (web `diffTruncationNote`). */
fun diffTruncationNote(lines: Int): String =
    "$lines more line${if (lines == 1) "" else "s"} truncated"
