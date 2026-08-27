package com.exponential.app.data.steer

import com.exponential.app.domain.AgentPhase

// The relay's close codes (apps/steer-relay/src/protocol.ts:351-361) — the
// three a viewer socket has to tell apart. Everything else (4002 replaced,
// 4009 publisher idle, a plain 1006 the radio caused) is an ordinary drop and
// takes the backoff path.

/** The room is over — the publisher said so. Nothing to redial. */
const val CLOSE_SESSION_ENDED = 4001

/** The ticket was refused. Retrying mints the same "no" forever. */
const val CLOSE_UNAUTHORIZED = 4003

/**
 * The relay dropped us because our socket fell behind its send buffer — a
 * chatty agent outrunning a phone radio, not a fault. The room is still there
 * and the join replay costs nothing, so this redials AT ONCE and stays
 * invisible: no backoff, no attempt counted, no "Connection lost" banner
 * (EXP-621 — a busy session used to flap that banner every few seconds).
 */
const val CLOSE_SLOW_CONSUMER = 4008

/** What a closed viewer socket should do next — the whole close-code policy. */
sealed interface SteerCloseAction {
    /** Redial immediately, silently: no backoff, no attempt, no banner. */
    data object RedialNow : SteerCloseAction

    /** The session is over; park on the ended state. */
    data object Ended : SteerCloseAction

    /** A "no" that can never turn into a yes — stop dialing. */
    data object Terminal : SteerCloseAction

    /** An ordinary drop: auto-reconnect on jittered exponential backoff. */
    data object Backoff : SteerCloseAction
}

/**
 * Decide what a dial's close code means. [sessionOver] is the synced
 * coding_sessions row's verdict (ended, or gone) — a slow-consumer drop on a
 * session that has since finished has nothing left to reconnect to.
 */
fun steerCloseAction(code: Int?, sessionOver: Boolean): SteerCloseAction = when (code) {
    CLOSE_SESSION_ENDED -> SteerCloseAction.Ended
    CLOSE_UNAUTHORIZED -> SteerCloseAction.Terminal
    CLOSE_SLOW_CONSUMER ->
        if (sessionOver) SteerCloseAction.Ended else SteerCloseAction.RedialNow
    else -> SteerCloseAction.Backoff
}

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
 * received in the stale window. There is no pong to probe with (ktor's OkHttp
 * engine rejects an outgoing Ping frame outright and kills the socket), so a
 * silent redial is the probe.
 */
fun steerRevivalAction(
    phase: AgentPhase,
    dialActive: Boolean,
    finished: Boolean,
    socketStale: Boolean,
): SteerRevivalAction = when {
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
