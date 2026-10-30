package com.exponential.app.data.steer

import com.exponential.app.data.api.SteerTicketResult
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.PeerChat
import com.exponential.app.data.enet.PeerNode
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.DomainContract
import java.io.File
import java.time.Instant
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * VAPP-33: the Android steer viewer over eNet. The fixture's `parse` cases
 * and constants, the transport choice (forSession order, no_such_session →
 * the next endpoint, every fallback to the relay), the viewer-side filter,
 * the re-join after staleMs, and a [SteerConnection] fed over eNet.
 */
class SteerEnetTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.steer.enetFixture")
            ?: error("exponential.steer.enetFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private val fast = SteerEnetTimings(joinTimeoutMs = 400, staleMs = 200, sendTimeoutMs = 400)

    private fun frame(text: String) = Json.parseToJsonElement(text).jsonObject

    /** A viewer node; [device] answers what it is sent, as the device side would. */
    private class FakeNode(override val endpointId: String = "viewer") : PeerNode {
        private val flow = MutableSharedFlow<PeerChat>(extraBufferCapacity = 256)
        override val chats = flow
        val sent = CopyOnWriteArrayList<Pair<String, JsonObject>>()
        @Volatile var device: FakeNode.(peer: String, envelope: SteerEnvelope) -> Unit = { _, _ -> }
        @Volatile var refuse: (String) -> Boolean = { false }

        override fun allowedPeers(): Set<String> = emptySet()
        override fun allow(peer: String) = Unit
        override fun revoke(peer: String) = Unit
        override suspend fun shutdown() = Unit

        override suspend fun sendChat(peer: String, body: ByteArray) {
            if (refuse(peer)) throw IllegalStateException("refused")
            val envelope = parseSteerEnvelope(body.decodeToString()) ?: error("not an envelope")
            envelope.frames.forEach { sent += peer to it }
            device(peer, envelope)
        }

        fun deliver(from: String, sessionId: String, vararg frames: String) {
            val body = encodeSteerEnvelope(sessionId, frames.map { Json.parseToJsonElement(it).jsonObject })
            check(flow.tryEmit(PeerChat(from, body)))
        }

        fun sentTypes(peer: String) = sent.filter { it.first == peer }.map { it.second["t"]!!.jsonPrimitive.content }
    }

    private val replay = arrayOf(
        """{"t":"activity_reset"}""",
        """{"t":"activity","seq":0,"event":{"kind":"narration","id":"n1","text":"hi"}}""",
        """{"t":"activity_synced","firstSeq":0,"lastSeq":0}""",
    )

    /** A device at [live] serving SESSION; every other endpoint runs no such session. */
    private fun FakeNode.serve(live: String) {
        device = { peer, envelope ->
            if (envelope.types.contains("join")) {
                if (peer == live) deliver(peer, envelope.sessionId, *replay)
                else deliver(peer, envelope.sessionId, """{"t":"error","code":"no_such_session"}""")
            }
        }
    }

    private fun dialer(node: PeerNode?, endpoints: List<String>, discoveries: AtomicInteger = AtomicInteger()) =
        EnetSteerDialer(
            node = { node },
            discover = { discoveries.incrementAndGet(); endpoints },
            timings = fast,
            dispatcher = Dispatchers.Default,
        )

    private suspend fun SteerSocket.next(): String = withTimeout(2_000) { incoming.receive() }

    private fun tag(text: String) = frame(text)["t"]!!.jsonPrimitive.content

    // ── the fixture ─────────────────────────────────────────────────────

    @Test
    fun parseReplaysTheFixture() {
        for (case in fixture["parse"]!!.jsonArray.map { it.jsonObject }) {
            val name = case["name"]!!.jsonPrimitive.content
            val body = case["bodyText"]?.jsonPrimitive?.content ?: case["body"]!!.toString()
            val parsed = parseSteerEnvelope(body)
            val ok = case["ok"]!!.jsonPrimitive.content.toBoolean()
            if (!ok) {
                assertNull(name, parsed)
                continue
            }
            assertNotNull(name, parsed)
            assertEquals(name, case["sessionId"]!!.jsonPrimitive.content, parsed!!.sessionId)
            assertEquals(name, case["types"]!!.jsonArray.map { it.jsonPrimitive.content }, parsed.types)
        }
    }

    @Test
    fun theConstantsAreTheFixtures() {
        val defaults = SteerEnetTimings()
        assertEquals(fixture["tag"]!!.jsonPrimitive.content, STEER_ENET_TAG)
        assertEquals(fixture["version"]!!.jsonPrimitive.long.toInt(), STEER_ENET_VERSION)
        assertEquals(fixture["joinTimeoutMs"]!!.jsonPrimitive.long, defaults.joinTimeoutMs)
        assertEquals(fixture["staleMs"]!!.jsonPrimitive.long, defaults.staleMs)
        assertEquals(fixture["sendTimeoutMs"]!!.jsonPrimitive.long, defaults.sendTimeoutMs)
    }

    @Test
    fun anEncodedEnvelopeParsesBackWithItsFramesUnchanged() {
        val frames = listOf(frame("""{"t":"input","data":"hello"}"""), frame("""{"t":"input","data":"\r"}"""))
        val parsed = parseSteerEnvelope(encodeSteerEnvelope("s1", frames).decodeToString())!!
        assertEquals("s1", parsed.sessionId)
        assertEquals(frames, parsed.frames)
    }

    // ── the transport choice ────────────────────────────────────────────

    @Test
    fun joinsTheFirstEndpointThatRunsTheSession() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-b") }
        val socket = dialer(node, listOf("dev-a", "dev-b"))!!.open(SESSION)!!
        try {
            assertEquals("dev-b", socket.endpoint)
            assertEquals(listOf("join"), node.sentTypes("dev-a"))
            assertEquals(listOf("join"), node.sentTypes("dev-b"))
            // The held replay is what the connection reads first, in order.
            assertEquals(listOf("activity_reset", "activity", "activity_synced"), List(3) { tag(socket.next()) })
        } finally {
            socket.cancel()
        }
    }

    @Test
    fun everyFallbackIsTheRelay() = runBlocking {
        // No node.
        assertNull(dialer(null, listOf("dev-a")).open(SESSION))
        // No endpoint.
        assertNull(dialer(FakeNode(), emptyList()).open(SESSION))
        // Discovery failing.
        val failing = EnetSteerDialer(node = { FakeNode() }, discover = { error("offline") }, timings = fast)
        assertNull(failing.open(SESSION))
        // Every endpoint refused: no_such_session everywhere, or a refused send.
        assertNull(dialer(FakeNode().apply { serve(live = "nobody") }, listOf("dev-a", "dev-b")).open(SESSION))
        assertNull(dialer(FakeNode().apply { refuse = { true } }, listOf("dev-a")).open(SESSION))
        // An unauthorized answer counts as refused.
        val unauthorized = FakeNode().apply {
            device = { peer, env -> deliver(peer, env.sessionId, """{"t":"error","code":"unauthorized"}""") }
        }
        assertNull(dialer(unauthorized, listOf("dev-a")).open(SESSION))
    }

    @Test
    fun noActivitySyncedWithinTheJoinTimeoutIsTheRelay() = runBlocking {
        // The device answers the reset but never ends its replay; the second
        // endpoint is never tried (the timeout falls straight to the relay).
        val node = FakeNode().apply {
            device = { peer, env -> if (peer == "dev-a") deliver(peer, env.sessionId, """{"t":"activity_reset"}""") }
        }
        val started = System.currentTimeMillis()
        assertNull(dialer(node, listOf("dev-a", "dev-b")).open(SESSION))
        assertTrue(System.currentTimeMillis() - started >= fast.joinTimeoutMs)
        assertEquals(emptyList<String>(), node.sentTypes("dev-b"))
    }

    // ── the viewer side ─────────────────────────────────────────────────

    @Test
    fun appliesOnlyTheJoinedSessionFromTheJoinedEndpoint() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        try {
            repeat(3) { socket.next() }
            node.deliver("dev-x", SESSION, """{"t":"activity","seq":9,"event":{"kind":"narration","id":"x","text":"spoofed"}}""")
            node.deliver("dev-a", "other-session", """{"t":"activity","seq":9,"event":{"kind":"narration","id":"y","text":"other"}}""")
            node.deliver("dev-a", SESSION, """{"t":"activity","seq":1,"event":{"kind":"narration","id":"n2","text":"live"}}""")
            assertEquals(1L, frame(socket.next())["seq"]!!.jsonPrimitive.long)
        } finally {
            socket.cancel()
        }
    }

    @Test
    fun everyClientFrameGoesOutUnchangedAndTheConnectionsJoinIsNotRepeated() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        try {
            val frames = listOf(
                """{"t":"join","channel":"activity"}""",
                """{"t":"input","data":"hello"}""",
                """{"t":"input","data":"\r"}""",
                """{"t":"answer","questionId":"q1","keys":["1"]}""",
                """{"t":"interrupt"}""",
                """{"t":"unqueue","id":"m1"}""",
                """{"t":"set_config","id":"model","value":"opus"}""",
                """{"t":"set_mode","id":"plan"}""",
                """{"t":"kill"}""",
                """{"t":"history_page","requestId":"r1","beforeSeq":5,"limit":50}""",
            )
            frames.forEach { socket.send(it) }
            // The probe's join already joined: the connection's own is swallowed.
            assertEquals(listOf(frame(frames[0])) + frames.drop(1).map(::frame), node.sent.filter { it.first == "dev-a" }.map { it.second })
        } finally {
            socket.cancel()
        }
    }

    @Test
    fun silenceForStaleMsReJoinsAndThenEndsTheFeed() = runBlocking {
        val joins = AtomicInteger()
        val node = FakeNode().apply {
            device = { peer, env ->
                if (env.types.contains("join") && joins.incrementAndGet() == 1) deliver(peer, env.sessionId, *replay)
            }
        }
        val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        repeat(3) { socket.next() }
        withTimeout(2_000) { while (joins.get() < 2) delay(10) }
        // Still silent a staleMs after the re-join: the feed closes, the loop redials.
        val end = withTimeoutOrNull(2_000) { socket.incoming.receiveCatching() }
        assertNotNull(end)
        assertTrue(end!!.isClosed)
        assertEquals("stale", socket.closeReason())
    }

    @Test
    fun aKeepaliveHoldsTheFeedAndAReJoinRestartsIt() = runBlocking {
        val joins = AtomicInteger()
        val node = FakeNode().apply {
            device = { peer, env -> if (env.types.contains("join")) { joins.incrementAndGet(); deliver(peer, env.sessionId, *replay) } }
        }
        val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        try {
            repeat(3) { socket.next() }
            repeat(4) {
                delay(fast.staleMs / 2)
                node.deliver("dev-a", SESSION, """{"t":"keepalive"}""")
                assertEquals("keepalive", tag(socket.next()))
            }
            assertEquals(1, joins.get())
            // Silence: the re-join's fresh replay arrives on the same feed.
            assertEquals("activity_reset", tag(withTimeout(2_000) { socket.incoming.receive() }))
            assertEquals(2, joins.get())
        } finally {
            socket.cancel()
        }
    }

    @Test
    fun aByeIsDeliveredAndEndsTheFeed() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        repeat(3) { socket.next() }
        node.deliver("dev-a", SESSION, """{"t":"bye","outcome":"ended"}""")
        assertEquals("bye", tag(socket.next()))
        assertTrue(withTimeout(2_000) { socket.incoming.receiveCatching() }.isClosed)
    }

    // ── the connection over eNet ────────────────────────────────────────

    private class EnetFirstTransport(private val dialer: EnetSteerDialer) : SteerTransport {
        val mints = AtomicInteger()
        override suspend fun mint(codingSessionId: String): SteerTicketResult {
            mints.incrementAndGet()
            return SteerTicketResult(ticket = null, url = null)
        }
        override suspend fun open(url: String): SteerSocket = error("the relay is not dialed")
        override suspend fun openEnet(codingSessionId: String): SteerSocket? = dialer.open(codingSessionId)
    }

    private fun connection(transport: SteerTransport) = SteerConnection(
        codingSessionId = SESSION,
        transport = transport,
        sessionFlow = MutableStateFlow(
            SessionEntity(
                id = SESSION,
                teamId = "team-1",
                userId = "user-1",
                status = DomainContract.sessionStatusRunning,
                startedAt = Instant.now().toString(),
                createdAt = Instant.now().toString(),
                updatedAt = Instant.now().toString(),
            ),
        ),
        json = Json { ignoreUnknownKeys = true; explicitNulls = false },
        dispatcher = Dispatchers.Unconfined,
        nowMs = { System.currentTimeMillis() },
        timings = SteerTimings(joinAckMs = 2_000, liveStaleMs = 10_000),
    )

    @Test
    fun theConnectionGoesLiveOverENetAndSteersThroughIt() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val transport = EnetFirstTransport(dialer(node, listOf("dev-a")))
        val connection = connection(transport)
        try {
            connection.connect()
            withTimeout(3_000) { while (connection.phase.value != AgentPhase.Live) delay(5) }
            // The same reducer the relay feeds: the replayed narration is on screen.
            withTimeout(3_000) {
                while (connection.activity.value.feed.none { it is AgentFeedItem.Narration }) delay(5)
            }
            assertEquals(0, transport.mints.get())
            assertTrue(connection.sendMessage("over enet"))
            connection.interrupt()
            withTimeout(3_000) { while (node.sentTypes("dev-a").count { it != "join" } < 3) delay(5) }
            assertEquals(listOf("join", "input", "input", "interrupt"), node.sentTypes("dev-a"))
            assertEquals("over enet", node.sent[1].second["data"]!!.jsonPrimitive.content)
        } finally {
            connection.close()
        }
    }

    @Test
    fun withoutAnENetFeedTheConnectionMintsTheRelayAsToday() = runBlocking {
        val transport = EnetFirstTransport(dialer(FakeNode(), emptyList()))
        val connection = connection(transport)
        try {
            connection.connect()
            withTimeout(3_000) { while (transport.mints.get() == 0) delay(5) }
        } finally {
            connection.close()
        }
    }

    private companion object {
        const val SESSION = "11111111-1111-4111-8111-111111111111"
    }
}
