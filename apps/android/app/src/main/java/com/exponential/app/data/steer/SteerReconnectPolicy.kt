package com.exponential.app.data.steer

import com.exponential.app.domain.AgentPhase

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

/** EXP-1487: what an empty transcript says while NO FEED holds (fixtures/steer-enet.json `noFeed`). */
enum class SteerNoFeedState { Offline, Starting, Idle }

/** EXP-1487: fixtures/steer-enet.json `noFeedIdle` (the fixture test pins them). */
internal const val STEER_NO_FEED_IDLE_TITLE = "No live transcript"
internal const val STEER_NO_FEED_IDLE_TEXT =
    "The device holding this session isn't publishing right now. Its conversation stays in the chat."

/**
 * EXP-1487 (fixtures/steer-enet.json "NO FEED, EMPTY TRANSCRIPT"): an ended
 * row = the device-offline copy, a busy row = starting, anything else (idle,
 * waiting on a question) = the idle empty state, never "starting".
 */
fun steerNoFeedState(ended: Boolean, busy: Boolean): SteerNoFeedState = when {
    ended -> SteerNoFeedState.Offline
    busy -> SteerNoFeedState.Starting
    else -> SteerNoFeedState.Idle
}
