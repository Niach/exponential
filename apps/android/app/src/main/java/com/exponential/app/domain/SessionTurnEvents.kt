package com.exponential.app.domain

// EXP-1245: the facts the OWNER's thread of turns reads off the relay feed
// they already hold ([sessionTurns]): their messages (`user_message` rows,
// main agent only) and the turn edges. The feed keeps the turn as a
// latest-wins SLOT, not as rows, so this records every edge it OBSERVES into a
// per-session log that outlives the screen: a `started` with its
// `startedAt`, an `ended` only when it is seen to follow a start (an `ended`
// slot found on arrival has no time to give). A message is stamped when it
// ARRIVES live (a lone new row), never a history replay's bulk (its times are
// unknown, so those rows stay out of the turns). The run's own start opens
// the first turn (its prompt is the issue or the composer's text, no bubble),
// so results published before any observed edge land there. Mirrors web
// `lib/session-turn-events.ts` (same names, same test names).

/** One feed row as the log reads it. [at] = the relay's stamp when kept. */
data class TurnFeedRow(
    val id: Long,
    val isUserMessage: Boolean,
    val text: String?,
    val subagentId: String? = null,
    val at: Long? = null,
)

class TurnLog {
    /** Recorded edges, in observation order. */
    val edges: MutableList<SessionTurnEvent.Turn> = mutableListOf()

    /** Feed row id → the time it is placed at. */
    val messageAt: LinkedHashMap<Long, Long> = linkedMapOf()

    /** Every feed row id seen so far. */
    val seen: MutableSet<Long> = mutableSetOf()
    var primed: Boolean = false
    var lastState: String? = null
    var lastStart: Long? = null
}

/** A lone new row (or two: an echo and its twin) is a live arrival. */
const val LIVE_ARRIVAL_MAX_ROWS = 2

fun emptyTurnLog(): TurnLog = TurnLog()

/** Fold the turn slot's current value ([TURN_STATE_STARTED] / [TURN_STATE_ENDED]) into the log. */
fun recordTurnSlot(log: TurnLog, state: String, startedAt: Long?, now: Long) {
    if (state == TURN_STATE_STARTED) {
        val at = startedAt ?: now
        if (log.lastState != TURN_STATE_STARTED || (startedAt != null && startedAt != log.lastStart)) {
            log.edges += SessionTurnEvent.Turn(started = true, at = at)
        }
        log.lastStart = startedAt ?: log.lastStart ?: at
    } else if (log.lastState == TURN_STATE_STARTED) {
        log.edges += SessionTurnEvent.Turn(started = false, at = maxOf(now, log.lastStart ?: now))
    }
    log.lastState = state
}

/** Fold the feed's rows into the log: place every main-agent message. */
fun recordFeedMessages(log: TurnLog, feed: List<TurnFeedRow>, now: Long) {
    val fresh = feed.filter { it.id !in log.seen }
    val live = log.primed && fresh.size <= LIVE_ARRIVAL_MAX_ROWS
    for (row in fresh) {
        log.seen += row.id
        if (!row.isUserMessage || row.subagentId != null) continue
        when {
            row.at != null -> log.messageAt[row.id] = row.at
            live -> log.messageAt[row.id] = now
        }
    }
    log.primed = true
}

/**
 * The events [sessionTurns] walks: the run's start, the placed messages
 * (still in the feed) and the recorded edges. Empty when nothing was observed
 * beyond the start (the single-row thread).
 */
fun turnEventsOf(log: TurnLog, feed: List<TurnFeedRow>, runStartedAt: Long?): List<SessionTurnEvent> {
    val messages = mutableListOf<SessionTurnEvent>()
    for (row in feed) {
        val at = log.messageAt[row.id] ?: continue
        val text = row.text
        if (!row.isUserMessage || text.isNullOrEmpty()) continue
        val parsed = parseSteerMessage(text)
        if (parsed.text.isEmpty() && parsed.attachmentIds.isEmpty() && parsed.files.isEmpty()) continue
        messages += SessionTurnEvent.UserMessage(
            at = at,
            text = parsed.text,
            images = parsed.attachmentIds.map { "/api/attachments/$it" },
            files = parsed.files,
        )
    }
    if (messages.isEmpty() && log.edges.isEmpty()) return emptyList()
    val first = runStartedAt?.let { listOf(SessionTurnEvent.Turn(started = true, at = it)) }.orEmpty()
    return first + messages + log.edges
}

/**
 * Web M6 ×4 (`firstTurnEndKnown`): whether the FIRST turn's end was actually
 * observed. [turnEventsOf] puts a synthetic `started` edge at the run's start
 * in front; that turn's end is known only when the first event after it is an
 * observed `started` edge. Otherwise ("Done on <device>") the caption drops
 * the duration rather than measure from a guessed end.
 */
fun firstTurnEndKnown(events: List<SessionTurnEvent>, runStartedAt: Long?): Boolean {
    val first = events.firstOrNull() ?: return true
    // No synthetic run start in front: every turn opened on an observed edge.
    if (first !is SessionTurnEvent.Turn || !first.started || runStartedAt == null || first.at != runStartedAt) {
        return true
    }
    val next = events.drop(1).withIndex()
        .sortedWith(compareBy({ it.value.at }, { it.index }))
        .firstOrNull()?.value
    return next == null || (next is SessionTurnEvent.Turn && next.started)
}

private val turnLogs = mutableMapOf<String, TurnLog>()

/** The per-session log, kept for the process like the steer connection. */
@Synchronized
fun turnLogFor(sessionId: String): TurnLog = turnLogs.getOrPut(sessionId) { TurnLog() }

// EXP-1245: the caption under the owner's bubble (web `userMessageCaption`,
// `user-message-bubble.tsx`): `<name> · <HH:mm> · from <device>`, a missing
// part dropping with its separator.

/** `HH:mm` in [zone], both zero-padded. */
fun userMessageTime(at: Long, zone: java.time.ZoneId = java.time.ZoneId.systemDefault()): String {
    val time = java.time.Instant.ofEpochMilli(at).atZone(zone)
    return "${time.hour.toString().padStart(2, '0')}:${time.minute.toString().padStart(2, '0')}"
}

fun userMessageCaption(
    name: String?,
    at: Long?,
    device: String?,
    zone: java.time.ZoneId = java.time.ZoneId.systemDefault(),
): String = buildList {
    name?.trim()?.takeIf { it.isNotEmpty() }?.let(::add)
    at?.let { add(userMessageTime(it, zone)) }
    device?.trim()?.takeIf { it.isNotEmpty() }?.let { add("from $it") }
}.joinToString(" · ")
