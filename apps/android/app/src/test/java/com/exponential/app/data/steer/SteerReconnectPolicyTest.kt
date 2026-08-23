package com.exponential.app.data.steer

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
}
