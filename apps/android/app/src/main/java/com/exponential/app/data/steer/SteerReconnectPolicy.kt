package com.exponential.app.data.steer

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
