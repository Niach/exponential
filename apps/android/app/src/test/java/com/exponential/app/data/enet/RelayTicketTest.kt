package com.exponential.app.data.enet

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-1471: the same vector as apps/desktop/crates/enet-core/tests/net.rs
// `relay_ticket_vector` (identity [7; 32] on the production relay).
class RelayTicketTest {
    private val id = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
    private val ticket = "enet15jfgyy7ctrjavpxvkb5rglwf7gkuo5vox27hxescd3vgsfcg2iwachlior2ha4z2f4xxezlmmf4s4zlyobxw4zlooruwc3bomf2c6aa"

    @Test fun encodesTheEnetCoreTicketOfARelayHomedPeer() {
        assertEquals(ticket, RelayTicket.encode(id))
        assertEquals(ticket, RelayTicket.encode(id, "https://relay.exponential.at/"))
        assertEquals(ticket, RelayTicket.encode(id.uppercase()))
    }

    @Test fun refusesAMalformedEndpointId() {
        assertNull(RelayTicket.encode("a1"))
    }
}
