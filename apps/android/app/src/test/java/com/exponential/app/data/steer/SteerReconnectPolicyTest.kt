package com.exponential.app.data.steer

import com.exponential.app.domain.AgentPhase
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-621: what a closed viewer socket does next is decided by the relay's
// close code (apps/steer-relay/src/protocol.ts), not by "it dropped, back off".
// The one that mattered is 4008 — slow-consumer eviction, which a chatty agent
// on a phone radio triggers routinely: it is not a fault, so it redials at once
// and stays invisible instead of flapping the reconnect banner every few
// seconds.
class SteerReconnectPolicyTest {

    @Test
    fun `slow consumer on a running session redials immediately`() {
        assertEquals(
            SteerCloseAction.RedialNow,
            steerCloseAction(CLOSE_SLOW_CONSUMER, sessionOver = false),
        )
    }

    @Test
    fun `slow consumer on a finished session does not redial`() {
        assertEquals(
            SteerCloseAction.Ended,
            steerCloseAction(CLOSE_SLOW_CONSUMER, sessionOver = true),
        )
    }

    @Test
    fun `unauthorized is terminal`() {
        assertEquals(
            SteerCloseAction.Terminal,
            steerCloseAction(CLOSE_UNAUTHORIZED, sessionOver = false),
        )
    }

    @Test
    fun `session ended closes the screen out`() {
        assertEquals(
            SteerCloseAction.Ended,
            steerCloseAction(CLOSE_SESSION_ENDED, sessionOver = false),
        )
    }

    @Test
    fun `a missing close code takes the backoff path`() {
        assertEquals(SteerCloseAction.Backoff, steerCloseAction(null, sessionOver = false))
    }

    @Test
    fun `an ordinary close takes the backoff path`() {
        // 1006 abnormal (radio died), 4002 replaced, 4009 publisher idle — all
        // plain drops the auto-reconnect handles.
        assertEquals(SteerCloseAction.Backoff, steerCloseAction(1006, sessionOver = false))
        assertEquals(SteerCloseAction.Backoff, steerCloseAction(4002, sessionOver = false))
        assertEquals(SteerCloseAction.Backoff, steerCloseAction(4009, sessionOver = false))
    }

    @Test
    fun `a finished session does not change the ordinary paths`() {
        // The session-over flag is consulted ONLY by the slow-consumer branch:
        // everything else already has its own exit (the dial loop re-checks the
        // synced row after its backoff).
        assertEquals(SteerCloseAction.Backoff, steerCloseAction(1006, sessionOver = true))
        assertEquals(SteerCloseAction.Terminal, steerCloseAction(CLOSE_UNAUTHORIZED, sessionOver = true))
    }

    // ── Revival policy (EXP-625) ────────────────────────────────────────────
    // The bug this encodes: revival used to be gated on the PHASE, so a dial
    // coroutine that died while the app was backgrounded left the screen on
    // "Connecting…" with no path back: nothing dials when the phase isn't
    // Idle, and nothing redials when it isn't a reconnecting Closed. The loop's
    // liveness is the truth; the phase only refines what to do while it lives.

    private fun revival(
        phase: AgentPhase,
        dialActive: Boolean,
        finished: Boolean = false,
        socketStale: Boolean = false,
    ) = steerRevivalAction(phase, dialActive, finished, socketStale)

    @Test
    fun `a dead dial loop is redialed under every non-final phase`() {
        // Connecting and Starting are the two that used to be absorbing.
        listOf(
            AgentPhase.Idle,
            AgentPhase.Connecting,
            AgentPhase.Starting,
            AgentPhase.Live,
            AgentPhase.Closed("dropped", reconnecting = true),
        ).forEach { phase ->
            assertEquals(phase.toString(), SteerRevivalAction.Dial, revival(phase, dialActive = false))
        }
    }

    @Test
    fun `a live loop waiting out a retry is woken instead of restarted`() {
        assertEquals(
            SteerRevivalAction.WakeRetry,
            revival(AgentPhase.Closed("dropped", reconnecting = true), dialActive = true),
        )
        assertEquals(SteerRevivalAction.WakeRetry, revival(AgentPhase.Starting, dialActive = true))
    }

    @Test
    fun `a fresh live socket is left alone`() {
        assertEquals(SteerRevivalAction.Nothing, revival(AgentPhase.Live, dialActive = true))
    }

    @Test
    fun `a silent live socket is redialed without a visible blip`() {
        assertEquals(
            SteerRevivalAction.RedialSilently,
            revival(AgentPhase.Live, dialActive = true, socketStale = true),
        )
    }

    @Test
    fun `connecting behind a live dial waits for it`() {
        // The dial is bounded now (upgrade + join-ack deadlines), so a second
        // one would only race the first.
        assertEquals(SteerRevivalAction.Nothing, revival(AgentPhase.Connecting, dialActive = true))
    }

    @Test
    fun `a finished connection is never revived`() {
        listOf(true, false).forEach { dialActive ->
            assertEquals(
                SteerRevivalAction.Nothing,
                revival(AgentPhase.Ended(), dialActive = dialActive, finished = true),
            )
            assertEquals(
                SteerRevivalAction.Nothing,
                revival(AgentPhase.Closed("steer is off"), dialActive = dialActive, finished = true),
            )
            // close() flips the flag before the phase settles, and the flag wins.
            assertEquals(
                SteerRevivalAction.Nothing,
                revival(AgentPhase.Connecting, dialActive = dialActive, finished = true),
            )
        }
    }
}
