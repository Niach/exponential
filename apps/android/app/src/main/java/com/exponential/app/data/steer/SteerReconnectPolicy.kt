package com.exponential.app.data.steer

import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.ChatExpress
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// VAPP-43: the viewer feed rides eNet only, so the relay's close-code policy
// (4001 ended, 4003 unauthorized, 4008 slow consumer) is gone with the relay
// socket; how a dial ends is decided in [SteerConnection] off its frames and
// the synced row.

/** What a REVIVAL entry (screen attach, app foreground, sync kick) should do
 *  to a connection: the whole "wake this thing up" policy (EXP-625). */
sealed interface SteerRevivalAction {
    /** Start a fresh dial loop. */
    data object Dial : SteerRevivalAction

    /** A loop is alive and waiting out a retry delay: cut the wait short. */
    data object WakeRetry : SteerRevivalAction

    /** Live on paper over a socket that has said nothing for too long: redial
     *  under the Live phase, so a dead radio heals without a visible blip. */
    data object RedialSilently : SteerRevivalAction

    /** Leave it alone. */
    data object Nothing : SteerRevivalAction
}

/**
 * Decide how to revive a viewer connection.
 *
 * EXP-625: every revival entry used to be PHASE-gated (dial only when Idle,
 * redial only when Closed-reconnecting), which made `Connecting` an absorbing
 * state: a dial coroutine that died or wedged in the background left the
 * screen on "Connecting…" forever, and nothing in the process could revive it.
 * The truth is the LOOP, not the phase: [dialActive] is `connectJob.isActive`,
 * and on Android that stays true for as long as a connection has any future
 * (the frame loop lives inside the dial). So a dead loop under ANY non-final
 * phase means redial, full stop.
 *
 * [socketStale] is the Live-phase liveness test: connected, but nothing
 * received in the stale window. The feed has no ping to probe with, so a
 * silent redial is the probe.
 */
fun steerRevivalAction(
    phase: AgentPhase,
    dialActive: Boolean,
    finished: Boolean,
    socketStale: Boolean,
    /** VAPP-169: the device-offline ended (an ended row, NO FEED). Not
     *  [finished]: a revival redials it (holding the phase). */
    deviceOffline: Boolean = false,
): SteerRevivalAction = when {
    deviceOffline -> if (dialActive) SteerRevivalAction.WakeRetry else SteerRevivalAction.Dial
    finished -> SteerRevivalAction.Nothing
    // Idle after a background park, or a loop that died under any phase.
    !dialActive -> SteerRevivalAction.Dial
    phase is AgentPhase.Closed && phase.reconnecting -> SteerRevivalAction.WakeRetry
    // Cut the 3s Starting wait short rather than sit it out.
    phase == AgentPhase.Starting -> SteerRevivalAction.WakeRetry
    phase == AgentPhase.Live && socketStale -> SteerRevivalAction.RedialSilently
    // Connecting behind a live (bounded) dial, and a fresh Live socket.
    else -> SteerRevivalAction.Nothing
}

/** VAPP-169: the rows [steerOfflineRedial] decides for
 *  (fixtures/steer-enet.json `offlineRedial`). */
enum class SteerRedialPhase {
    /** The device-offline ended: an ended row, NO FEED. */
    Offline,

    /** Ended by `bye`: the transcript is on screen. */
    Ended,
    Starting,
}

enum class SteerRedialTrigger { PresenceOnline, PresenceOffline, StaleTimer, Wakeup }

/**
 * VAPP-169 (fixtures/steer-enet.json "OFFLINE REDIAL"): whether [trigger]
 * redials a viewer in [phase]. The device-offline ended redials on every
 * trigger but the device going offline; a `bye`-ended transcript never does;
 * a Starting row cuts its backoff short on presence online or a wakeup and
 * otherwise keeps its own cadence (no stale timer).
 */
fun steerOfflineRedial(phase: SteerRedialPhase, trigger: SteerRedialTrigger): Boolean = when {
    trigger == SteerRedialTrigger.PresenceOffline || phase == SteerRedialPhase.Ended -> false
    phase == SteerRedialPhase.Offline -> true
    else -> trigger != SteerRedialTrigger.StaleTimer
}

/** EXP-1487: fixtures/steer-enet.json `lostBanner` (the fixture test pins them). */
internal const val STEER_LOST_GRACE_MS = 5_000L
internal const val STEER_LOST_FAILED_REDIALS = 2

/**
 * EXP-1487 (fixtures/steer-enet.json "LOST BANNER"): whether a live feed that
 * dropped shows "Connection lost". Until it has been down [downMs] >= the
 * grace AND [failedRedials] redials since the drop failed, it redials
 * silently, holding the phase.
 */
fun steerShowConnectionLost(
    downMs: Long,
    failedRedials: Int,
    graceMs: Long = STEER_LOST_GRACE_MS,
    minFailedRedials: Int = STEER_LOST_FAILED_REDIALS,
): Boolean = downMs >= graceMs && failedRedials >= minFailedRedials

/** EXP-1487: what an empty transcript says while NO FEED holds (fixtures/steer-enet.json `noFeed`);
 *  #EXP-1703: [Away] = a live row whose device is offline; #EXP-2000: [Waiting] = a row no
 *  device picked up yet (NEVER HELD: the placeholder says nothing). */
enum class SteerNoFeedState { Offline, Away, Starting, Idle, Waiting }

/** EXP-1487: fixtures/steer-enet.json `noFeedIdle` (the fixture test pins them). */
internal const val STEER_NO_FEED_IDLE_TITLE = "No live transcript"
internal const val STEER_NO_FEED_IDLE_TEXT =
    "The device holding this session isn't publishing right now. Its conversation stays in the chat."

/** #EXP-1703: fixtures/steer-enet.json `noFeedAway` (the fixture test pins them). */
internal const val STEER_NO_FEED_AWAY_TITLE = "{device} is offline"
internal const val STEER_NO_FEED_AWAY_TEXT = "This session continues when it's back. Its transcript loads then."
internal const val STEER_NO_FEED_AWAY_DEVICE = "Its device"

/** #EXP-1703: the empty state of a live row whose device is offline. */
data class SteerNoFeedAway(val title: String, val text: String)

/** #EXP-1703 (TS `steerNoFeedAway`): [deviceName] null/empty = "Its device". */
fun steerNoFeedAway(deviceName: String?): SteerNoFeedAway = SteerNoFeedAway(
    title = STEER_NO_FEED_AWAY_TITLE.replace("{device}", deviceName?.takeIf { it.isNotEmpty() } ?: STEER_NO_FEED_AWAY_DEVICE),
    text = STEER_NO_FEED_AWAY_TEXT,
)

/**
 * EXP-1487 (fixtures/steer-enet.json "NO FEED, EMPTY TRANSCRIPT"): an ended
 * row = the device-offline copy; #EXP-1703: a live row whose device is
 * offline ([deviceOffline]) = away; a busy row = starting; #EXP-2000 a row no
 * device picked up yet ([started] false: `startedAt` null) = waiting; anything
 * else (idle, waiting on a question) = the idle empty state, never "starting".
 */
fun steerNoFeedState(
    ended: Boolean,
    busy: Boolean,
    deviceOffline: Boolean = false,
    started: Boolean = true,
    /** #EXP-2119: its device is ONLINE in presence (unknown = false): an ended row has no transcript there. */
    deviceOnline: Boolean = false,
): SteerNoFeedState = when {
    ended && deviceOnline -> SteerNoFeedState.Idle
    ended -> SteerNoFeedState.Offline
    deviceOffline -> SteerNoFeedState.Away
    busy -> SteerNoFeedState.Starting
    !started -> SteerNoFeedState.Waiting
    else -> SteerNoFeedState.Idle
}

/** Android 0.14.134 (fixtures/steer-enet.json `noFeedInputsMax`, pinned by the fixture test). */
internal const val STEER_NO_FEED_INPUTS_MAX = 10

/** One synced message as [steerNoFeedInputs] reads it. */
data class SteerInputRow(
    val id: String,
    val authorKind: String,
    /** The row carries a card (`card` set and not JSON null). */
    val hasCard: Boolean,
    val consumedAt: String?,
    val createdAt: String,
)

/**
 * Android 0.14.134 (fixtures/steer-enet.json "SENT INPUTS", TS
 * `steerNoFeedInputs`): what a no-feed placeholder of a live row draws as
 * the viewer's own bubbles: the session's card-less person inputs the
 * device already CONSUMED (a not-consumed one is the Dock's queue), oldest
 * first (createdAt, then id), the newest [max]. The brief a claim consumes
 * at once is the first, so a session started here shows what was sent
 * before its device publishes.
 */
fun <T> steerNoFeedInputs(rows: List<T>, max: Int = STEER_NO_FEED_INPUTS_MAX, row: (T) -> SteerInputRow): List<T> {
    val inputs = rows
        .map { it to row(it) }
        .filter { (_, r) -> r.authorKind == "user" && !r.hasCard && r.consumedAt != null }
        .sortedWith(compareBy<Pair<T, SteerInputRow>>({ it.second.createdAt }, { it.second.id }))
        .map { it.first }
    return inputs.takeLast(max.coerceAtLeast(0))
}

/** #FEED-260 (fixtures/steer-enet.json `syncedTailMax`, pinned by the fixture test). */
internal const val STEER_SYNCED_TAIL_MAX = 50

/** One feed row as [steerSyncedTail] reads it: a main-lane narration
 *  (`agent`), an input (`user`) or anything else (null), its text, its
 *  known time (epoch ms; null = unknown). */
data class SteerTailFeedRow(val author: String?, val text: String?, val at: Long?)

/** One synced message as [steerSyncedTail] reads it. */
data class SteerTailMessage(
    val id: String,
    val authorKind: String,
    val hasCard: Boolean,
    val consumedAt: String?,
    val createdAt: String,
    /** Epoch ms of [createdAt]; null = unparseable. */
    val createdAtMs: Long?,
    /** The OPENED body. */
    val body: String,
    /** #EXP-2075: the row's `action` card when it reads ([steerTailActionCard]); [hasCard] stays "any card". */
    val action: ChatExpress.ActionCard? = null,
    /** #EXP-2084: the session that posted it (a child's reply), when not the chat's own. */
    val authorSessionId: String? = null,
)

/** #EXP-2075: a stored card as THE `action` card when it reads (kind
 *  `action`, a string name and surfaceId), else null. */
fun steerTailActionCard(card: JsonObject?): ChatExpress.ActionCard? =
    ChatExpress.ActionCard.of(card)?.takeIf { (card?.get("surfaceId") as? JsonPrimitive)?.isString == true }

private fun samePress(a: ChatExpress.ActionCard, b: ChatExpress.ActionCard): Boolean =
    a.name == b.name && a.surfaceId == b.surfaceId && a.block == b.block && a.messageId == b.messageId && a.context == b.context

/**
 * #FEED-260 (fixtures/steer-enet.json "SYNCED TAIL", TS `steerSyncedTail`):
 * the synced messages a NOT-live transcript draws after its feed, oldest
 * first (createdAt, then id), the newest [max]: card-less with a non-blank
 * body, the agent's or a person's CONSUMED input, later than the feed's
 * newest known row time, not already held by the feed (an agent line equal
 * to, a prefix of or prefixed by a narration; an input equal to a feed
 * input; #EXP-2084 another session's line the feed holds as that session's
 * input, whatever its time). #EXP-2075: a person's CONSUMED row with THE
 * `action` card is an input too, with or without a body; a feed input that
 * parses with [ChatExpress.parseActionInput] is that press (its body the
 * parsed one; a bodiless press compared by its card). An empty feed takes the
 * tail only while its live transcript cannot be loaded ([unreachable], the
 * OFFLINE LINE's condition) and only when it holds an agent line.
 */
fun <T> steerSyncedTail(
    live: Boolean,
    feed: List<SteerTailFeedRow>,
    messages: List<T>,
    max: Int = STEER_SYNCED_TAIL_MAX,
    unreachable: Boolean = false,
    row: (T) -> SteerTailMessage,
): List<T> {
    if (live) return emptyList()
    if (feed.isEmpty() && !unreachable) return emptyList()
    val feedAt = feed.mapNotNull { it.at }.maxOrNull()
    if (feed.isNotEmpty() && feedAt == null) return emptyList()
    val narrations = feed.filter { it.author == "agent" && !it.text.isNullOrEmpty() }.map { it.text!!.trim() }
    val inputs = HashSet<String>()
    val presses = ArrayList<ChatExpress.ActionCard>()
    // #EXP-2084: the inputs the device worded as another session's
    // (`Message from <name> (session <id>): <body>`), by session and body.
    val relays = HashSet<String>()
    for (r in feed) {
        if (r.author != "user" || r.text.isNullOrEmpty()) continue
        com.exponential.app.domain.TranscriptGrouping.crossSessionInput(r.text)?.let { relays += "${it.sessionId}\n${it.body.trim()}" }
        val action = ChatExpress.parseActionInput(r.text)
        val body = (action?.body ?: r.text).trim()
        if (body.isNotEmpty()) inputs += body else if (action != null) presses += action.card
    }
    val tail = messages
        .map { it to row(it) }
        .filter { (_, m) ->
            val action = if (m.hasCard && m.authorKind == "user") m.action else null
            if (m.hasCard && action == null) return@filter false
            val body = m.body.trim()
            when {
                action != null && body.isEmpty() ->
                    if (m.consumedAt == null || presses.any { samePress(it, action) }) return@filter false
                body.isEmpty() -> return@filter false
                m.authorKind == "agent" -> {
                    if (!m.authorSessionId.isNullOrEmpty() && "${m.authorSessionId}\n$body" in relays) return@filter false
                    if (narrations.any { it.isNotEmpty() && (it.startsWith(body) || body.startsWith(it)) }) return@filter false
                }
                m.authorKind == "user" -> if (m.consumedAt == null || body in inputs) return@filter false
                else -> return@filter false
            }
            feedAt == null || (m.createdAtMs != null && m.createdAtMs > feedAt)
        }
        .sortedWith(compareBy<Pair<T, SteerTailMessage>>({ it.second.createdAt }, { it.second.id }))
    if (feed.isEmpty() && tail.none { it.second.authorKind == "agent" }) return emptyList()
    return tail.takeLast(max.coerceAtLeast(0)).map { it.first }
}

/**
 * #FEED-260 (fixtures/steer-enet.json "OFFLINE LINE", TS `steerOfflineLine`):
 * a not-live transcript showing rows ends with THE inline marker
 * `noFeedAway.title`: an ended row whose last dial said NO FEED, a row not
 * ended whose device is offline in presence.
 */
fun steerOfflineLine(live: Boolean, rows: Int, ended: Boolean, noFeed: Boolean, deviceOffline: Boolean, deviceOnline: Boolean = false): Boolean = when {
    live || rows == 0 || deviceOnline -> false
    ended -> noFeed
    else -> deviceOffline
}

/** The synced row fields [steerRowWakes] reads. */
data class SteerWakeRow(val status: String, val busy: Boolean, val deviceId: String?)

/**
 * Android 0.14.134 (fixtures/steer-enet.json "ROW WAKE", TS `steerRowWakes`):
 * whether a synced row change cuts a Starting row's backoff short: the
 * device took it (status leaves `queued` or turns `running`), a turn
 * started (`busy` turned true), or it moved (`deviceId` changed). Never an
 * ending, never the first row seen (no edge).
 */
fun steerRowWakes(from: SteerWakeRow?, to: SteerWakeRow): Boolean = when {
    from == null || to.status == "ended" -> false
    from.status == "queued" && to.status != "queued" -> true
    from.status != "running" && to.status == "running" -> true
    !from.busy && to.busy -> true
    else -> from.deviceId.orEmpty() != to.deviceId.orEmpty()
}
