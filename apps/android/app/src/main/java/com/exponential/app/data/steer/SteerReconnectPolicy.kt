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
