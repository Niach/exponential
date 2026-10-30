package com.exponential.app.data.enet

import at.exponential.net.ffi.EnetEvent
import at.exponential.net.ffi.EnetException
import at.exponential.net.ffi.EnetNode
import at.exponential.net.ffi.defaultConfig
import com.exponential.app.data.api.RegisterInstallInput
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * EXP-1457 step 1 acceptance on REAL eNet nodes (the facade's host build via
 * JNA, localhost only, no relay): two installs of one team, admitted by the
 * allowlist `installs.list` feeds them, exchange an acked chat ping; an
 * install of a foreign team, and a revoked one, are refused.
 */
class InstallNodeEnetTest {
    /** The registry's contract (apps/web installs.ts), in memory. */
    private class FakeRegistry {
        val teams = mapOf("team-1" to setOf("u1", "u2"), "team-2" to setOf("u3"))
        val installs = mutableMapOf<String, Pair<String, Boolean>>() // endpointId -> (userId, revoked)

        fun register(userId: String, input: RegisterInstallInput): String {
            val message = installMessage(userId, input.endpointId, input.x25519Public, input.signedAt)
            check(verifiesAgainstEndpoint(input.endpointId, message, input.signature)) { "bad signature" }
            installs[input.endpointId]?.let { (owner, revoked) -> check(owner == userId && !revoked) }
            installs[input.endpointId] = userId to false
            return "install-${input.endpointId.take(8)}"
        }

        fun list(teamId: String): List<String> =
            installs.filter { (_, v) -> v.first in teams.getValue(teamId) && !v.second }.keys.toList()
    }

    private val local = defaultConfig().copy(relaysEnabled = false, relayUrls = emptyList(), n0Relays = false, bindAddr = "127.0.0.1:0")

    private fun install(registry: FakeRegistry, userId: String, started: MutableList<FfiPeerNode>) = InstallNode(
        userId = userId,
        identity = EnetFfi.generateIdentity(),
        crypto = EnetFfi,
        label = "test",
        platform = "JVM",
        register = { registry.register(userId, it) },
        listTeam = { registry.list(it) },
        startNode = { id -> FfiPeerNode.start(id, local).also(started::add) },
    )

    private val InstallNode.enet: EnetNode get() = (node as FfiPeerNode).node

    @Test
    fun teammatesExchangeAnAckedPingAndForeignOrRevokedEndpointsAreRefused() = runBlocking {
        val registry = FakeRegistry()
        val started = mutableListOf<FfiPeerNode>()
        val a = install(registry, "u1", started)
        val b = install(registry, "u2", started)
        val foreign = install(registry, "u3", started)
        try {
            a.start(); b.start(); foreign.start()
            assertEquals(3, registry.installs.size)
            assertTrue(a.refreshAccess(setOf("team-1")))
            assertTrue(b.refreshAccess(setOf("team-1")))
            assertTrue(foreign.refreshAccess(setOf("team-2")))
            val aId = a.enet.endpointId()
            val bId = b.enet.endpointId()
            assertEquals(setOf(bId), a.enet.allowedPeers().toSet())
            assertEquals(setOf(aId), b.enet.allowedPeers().toSet())
            assertTrue(foreign.enet.allowedPeers().isEmpty())

            // Teammates: an acked ping, delivered as a chat event.
            b.enet.addTicket(a.enet.ticket())
            val ack = withTimeout(20_000) { b.enet.sendChat(aId, "ping".toByteArray()) }
            assertEquals("ping", ack.body.decodeToString())
            val got = withTimeout(10_000) {
                var event = a.enet.nextEvent()
                while (event !is EnetEvent.Chat) event = a.enet.nextEvent()
                event
            }
            assertEquals(bId, got.from)
            assertEquals("ping", got.message.body.decodeToString())

            // A foreign team's install is refused.
            foreign.enet.addTicket(a.enet.ticket())
            assertRefused { foreign.enet.sendChat(aId, "hi".toByteArray()) }

            // A revoked install drops out at the next refresh and is refused too.
            registry.installs[bId] = "u2" to true
            assertTrue(a.refreshAccess(setOf("team-1")))
            assertFalse(bId in a.enet.allowedPeers())
            assertRefused { b.enet.sendChat(aId, "again".toByteArray()) }
        } finally {
            a.stop(); b.stop(); foreign.stop()
        }
    }

    @Test
    fun aFailedTeamListKeepsItsPreviousPeers() = runBlocking {
        val registry = FakeRegistry()
        val started = mutableListOf<FfiPeerNode>()
        val b = install(registry, "u2", started).also { it.start() }
        var failing = false
        val a = InstallNode(
            userId = "u1",
            identity = EnetFfi.generateIdentity(),
            crypto = EnetFfi,
            label = null,
            platform = null,
            register = { registry.register("u1", it) },
            listTeam = { if (failing) error("offline") else registry.list(it) },
            startNode = { id -> FfiPeerNode.start(id, local) },
        )
        try {
            a.start()
            assertTrue(a.refreshAccess(setOf("team-1")))
            failing = true
            assertFalse(a.refreshAccess(setOf("team-1")))
            assertEquals(setOf(b.enet.endpointId()), a.enet.allowedPeers().toSet())
            // Leaving the team drops its peers even while lists fail.
            assertTrue(a.refreshAccess(emptySet()))
            assertTrue(a.enet.allowedPeers().isEmpty())
        } finally {
            a.stop(); b.stop()
        }
    }

    private suspend fun assertRefused(block: suspend () -> Unit) {
        try {
            withTimeout(20_000) { block() }
            fail("expected EnetException.Refused")
        } catch (err: EnetException.Refused) {
            // expected
        }
    }
}
