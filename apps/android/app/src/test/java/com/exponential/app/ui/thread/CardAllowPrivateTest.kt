package com.exponential.app.ui.thread

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** VAPP-107: the plan card's surface host opts EXACTLY its own API host in. */
class CardAllowPrivateTest {

    @Test
    fun aLanInstanceOptsInOnlyItsOwnHost() {
        assertEquals(listOf("192.168.1.20"), cardAllowPrivate("http://192.168.1.20:3000"))
        assertEquals(listOf("exponential.local"), cardAllowPrivate("https://Exponential.local/"))
        assertEquals(listOf("::1"), cardAllowPrivate("http://[::1]:3000"))
    }

    @Test
    fun neverTheWholeLan() {
        val hosts = cardAllowPrivate("http://10.0.2.2:3000").orEmpty()
        assertEquals(listOf("10.0.2.2"), hosts)
        assert(hosts.none { it == "*" || it.startsWith("*.") })
    }

    @Test
    fun noOriginNoOptIn() {
        assertNull(cardAllowPrivate(null))
        assertNull(cardAllowPrivate("not a url"))
    }
}
