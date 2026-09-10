package com.exponential.app.data.steer

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-812: the capture-only authority rewrite. The whole point is that the
 * TICKET — which rides the query string — survives, so a wrong rewrite would
 * look like a relay that rejects every dial.
 */
class SteerTestHooksTest {

    @Test
    fun `keeps the path and the ticket query`() {
        assertEquals(
            "ws://10.0.2.2:4002/ws?ticket=abc.def",
            withRelayAuthority("ws://localhost:4002/ws?ticket=abc.def", "ws://10.0.2.2:4002"),
        )
    }

    @Test
    fun `rewrites scheme and port too`() {
        assertEquals(
            "ws://10.0.2.2:4002/ws?ticket=t",
            withRelayAuthority("wss://relay.exponential.at/ws?ticket=t", "ws://10.0.2.2:4002"),
        )
    }

    @Test
    fun `a portless authority keeps no port`() {
        assertEquals(
            "wss://relay.example.com/ws?ticket=t",
            withRelayAuthority("ws://localhost:4002/ws?ticket=t", "wss://relay.example.com"),
        )
    }

    @Test
    fun `an unusable authority leaves the minted url alone`() {
        val minted = "ws://localhost:4002/ws?ticket=t"
        assertEquals(minted, withRelayAuthority(minted, "10.0.2.2:4002"))
        assertEquals(minted, withRelayAuthority(minted, ""))
        assertEquals(minted, withRelayAuthority(minted, "not a uri"))
    }
}
