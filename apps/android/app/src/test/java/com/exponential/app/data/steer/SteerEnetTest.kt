package com.exponential.app.data.steer

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.PeerChat
import com.exponential.app.data.enet.PeerNode
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.HistoryState
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
import kotlinx.serialization.json.boolean
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
 * the next endpoint, every NO FEED case), the viewer-side filter, the re-join
 * after staleMs, and a [SteerConnection] fed over eNet only (VAPP-43: no
 * relay fallback; the two no-feed outcomes and an ended row's stored replay).
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
        val drops = MutableSharedFlow<String>(extraBufferCapacity = 16)
        override val disconnects = drops
        val sent = CopyOnWriteArrayList<Pair<String, JsonObject>>()
        @Volatile var device: FakeNode.(peer: String, envelope: SteerEnvelope) -> Unit = { _, _ -> }
        @Volatile var refuse: (String) -> Boolean = { false }

        @Volatile var paths: Map<String, String> = emptyMap()
        override fun pathKind(peer: String): String? = paths[peer]

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
    fun theOfflineRedialCasesAreTheFixtures() {
        // VAPP-169: fixtures/steer-enet.json `offlineRedial`.
        val cases = fixture["offlineRedial"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val phase = SteerRedialPhase.entries.single {
                it.name.equals(case["phase"]!!.jsonPrimitive.content, ignoreCase = true)
            }
            val trigger = SteerRedialTrigger.entries.single {
                it.name.equals(case["trigger"]!!.jsonPrimitive.content, ignoreCase = true)
            }
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                case["redial"]!!.jsonPrimitive.boolean,
                steerOfflineRedial(phase, trigger),
            )
        }
    }

    @Test
    fun theLostBannerCasesAreTheFixtures() {
        // EXP-1487: fixtures/steer-enet.json `lostBanner` + `lostBannerCases`.
        val lostBanner = fixture["lostBanner"]!!.jsonObject
        assertEquals(lostBanner["graceMs"]!!.jsonPrimitive.long, STEER_LOST_GRACE_MS)
        assertEquals(lostBanner["failedRedials"]!!.jsonPrimitive.long.toInt(), STEER_LOST_FAILED_REDIALS)
        assertEquals(STEER_LOST_GRACE_MS, SteerTimings().lostGraceMs)
        assertEquals(STEER_LOST_FAILED_REDIALS, SteerTimings().lostFailedRedials)
        val cases = fixture["lostBannerCases"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                case["show"]!!.jsonPrimitive.boolean,
                steerShowConnectionLost(
                    case["downMs"]!!.jsonPrimitive.long,
                    case["failedRedials"]!!.jsonPrimitive.long.toInt(),
                ),
            )
        }
    }

    @Test
    fun theNoFeedCasesAreTheFixtures() {
        // EXP-1487: fixtures/steer-enet.json `noFeedIdle` + `noFeed`.
        val idle = fixture["noFeedIdle"]!!.jsonObject
        assertEquals(idle["title"]!!.jsonPrimitive.content, STEER_NO_FEED_IDLE_TITLE)
        assertEquals(idle["text"]!!.jsonPrimitive.content, STEER_NO_FEED_IDLE_TEXT)
        // #EXP-1703: `noFeedAway`.
        val away = fixture["noFeedAway"]!!.jsonObject
        assertEquals(away["title"]!!.jsonPrimitive.content, STEER_NO_FEED_AWAY_TITLE)
        assertEquals(away["text"]!!.jsonPrimitive.content, STEER_NO_FEED_AWAY_TEXT)
        assertEquals(away["device"]!!.jsonPrimitive.content, STEER_NO_FEED_AWAY_DEVICE)
        assertEquals(
            SteerNoFeedAway("macbook is offline", away["text"]!!.jsonPrimitive.content),
            steerNoFeedAway("macbook"),
        )
        assertEquals("Its device is offline", steerNoFeedAway(null).title)
        assertEquals("Its device is offline", steerNoFeedAway("").title)
        val cases = fixture["noFeed"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val expected = when (val state = case["state"]!!.jsonPrimitive.content) {
                "offline" -> SteerNoFeedState.Offline
                "away" -> SteerNoFeedState.Away
                "starting" -> SteerNoFeedState.Starting
                "idle" -> SteerNoFeedState.Idle
                else -> error("unknown noFeed state $state")
            }
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                expected,
                steerNoFeedState(
                    ended = case["ended"]!!.jsonPrimitive.boolean,
                    busy = case["busy"]!!.jsonPrimitive.boolean,
                    deviceOffline = case["deviceOffline"]?.jsonPrimitive?.boolean ?: false,
                ),
            )
        }
    }

    @Test
    fun theNoFeedInputCasesAreTheFixtures() {
        // Android 0.14.134: fixtures/steer-enet.json SENT INPUTS.
        assertEquals(fixture["noFeedInputsMax"]!!.jsonPrimitive.long.toInt(), STEER_NO_FEED_INPUTS_MAX)
        val cases = fixture["noFeedInputCases"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val rows = case["messages"]!!.jsonArray.map { it.jsonObject }
            val picked = steerNoFeedInputs(rows, case["max"]!!.jsonPrimitive.long.toInt()) { m ->
                SteerInputRow(
                    id = m["id"]!!.jsonPrimitive.content,
                    authorKind = m["authorKind"]!!.jsonPrimitive.content,
                    hasCard = m["card"] !is kotlinx.serialization.json.JsonNull && m["card"] != null,
                    consumedAt = (m["consumedAt"] as? kotlinx.serialization.json.JsonPrimitive)
                        ?.takeIf { it.isString }?.content,
                    createdAt = m["createdAt"]!!.jsonPrimitive.content,
                )
            }
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                case["expected"]!!.jsonArray.map { it.jsonPrimitive.content },
                picked.map { it["id"]!!.jsonPrimitive.content },
            )
        }
    }

    @Test
    fun theRowWakeCasesAreTheFixtures() {
        // Android 0.14.134: fixtures/steer-enet.json ROW WAKE.
        fun row(value: JsonElement?): SteerWakeRow? = (value as? JsonObject)?.let { o ->
            SteerWakeRow(
                status = o["status"]!!.jsonPrimitive.content,
                busy = o["busy"]!!.jsonPrimitive.boolean,
                deviceId = (o["deviceId"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content,
            )
        }
        val cases = fixture["rowWakeCases"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                case["wake"]!!.jsonPrimitive.boolean,
                steerRowWakes(row(case["from"]), row(case["to"])!!),
            )
        }
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
    fun everyNoFeedCaseIsNull() = runBlocking {
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
    fun noActivitySyncedWithinTheJoinTimeoutIsNoFeed() = runBlocking {
        // The device answers the reset but never ends its replay; the second
        // endpoint is never tried (the timeout is NO FEED at once).
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

    // ── EXP-1487 SUBSCRIPTIONS / EXP-1531 a lost peer ───────────────────

    @Test
    fun cancellingWhileFramesAreStillQueuedNeverThrows() = runBlocking {
        // cancel() closes the read side while the pump still drains queued
        // frames: a plain send then threw ClosedSendChannelException into a
        // handler-less scope, which on a phone is the process's crash handler.
        val uncaught = CopyOnWriteArrayList<Throwable>()
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { _, e -> uncaught += e }
        try {
            val burst = Array(2_000) { """{"t":"activity","seq":${it + 1},"event":{"kind":"narration","id":"n$it","text":"x"}}""" }
            repeat(30) {
                val node = FakeNode().apply { serve(live = "dev-a") }
                val socket = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
                node.deliver("dev-a", SESSION, *burst)
                delay(1)
                socket.cancel()
            }
            delay(300)
            assertEquals(emptyList<Throwable>(), uncaught.toList())
        } finally {
            Thread.setDefaultUncaughtExceptionHandler(previous)
        }
    }

    @Test
    fun onlyTheLastSocketOfASessionAndDeviceSendsLeave() = runBlocking {
        // A replaced dial cancelled while the newer one is joined must not
        // end the newer feed: the device keys viewers by endpoint.
        val node = FakeNode().apply { serve(live = "dev-a") }
        val first = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        val second = dialer(node, listOf("dev-a"))!!.open(SESSION)!!
        first.cancel()
        delay(150)
        assertEquals(listOf("join", "join"), node.sentTypes("dev-a"))
        // The newer feed still gets the device's frames.
        repeat(3) { second.next() }
        node.deliver("dev-a", SESSION, """{"t":"keepalive"}""")
        assertEquals("keepalive", tag(second.next()))
        second.cancel()
        withTimeout(2_000) { while (node.sentTypes("dev-a").size < 3) delay(5) }
        assertEquals(listOf("join", "join", "leave"), node.sentTypes("dev-a"))
    }

    @Test
    fun aDisconnectOfTheJoinedPeerEndsTheFeedAtOnce() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        // staleMs far beyond the test: only the disconnect can end it.
        val slow = EnetSteerDialer(
            node = { node },
            discover = { listOf("dev-a") },
            timings = fast.copy(staleMs = 60_000),
            dispatcher = Dispatchers.Default,
        )
        val socket = slow.open(SESSION)!!
        try {
            repeat(3) { socket.next() }
            // Another peer's disconnect is not this feed's.
            check(node.drops.tryEmit("dev-b"))
            delay(50)
            node.deliver("dev-a", SESSION, """{"t":"keepalive"}""")
            assertEquals("keepalive", tag(socket.next()))
            check(node.drops.tryEmit("dev-a"))
            assertTrue(withTimeout(1_000) { socket.incoming.receiveCatching() }.isClosed)
            assertEquals("peer disconnected", socket.closeReason())
        } finally {
            socket.cancel()
        }
        // An ended feed sends no leave.
        delay(100)
        assertEquals(listOf("join"), node.sentTypes("dev-a"))
    }

    // ── the connection over eNet ────────────────────────────────────────

    private fun row(status: String) = SessionEntity(
        id = SESSION,
        teamId = "team-1",
        userId = "user-1",
        status = status,
        startedAt = Instant.now().toString(),
        createdAt = Instant.now().toString(),
        updatedAt = Instant.now().toString(),
    )

    private fun connection(
        transport: SteerTransport,
        status: String = DomainContract.sessionStatusRunning,
    ) = SteerConnection(
        codingSessionId = SESSION,
        transport = transport,
        sessionFlow = MutableStateFlow(row(status)),
        json = Json { ignoreUnknownKeys = true; explicitNulls = false },
        dispatcher = Dispatchers.Unconfined,
        nowMs = { System.currentTimeMillis() },
        timings = SteerTimings(joinAckMs = 2_000, liveStaleMs = 10_000, startingRetryMs = 50),
    )

    @Test
    fun theConnectionGoesLiveOverENetAndSteersThroughIt() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val connection = connection(EnetSteerTransport(dialer(node, listOf("dev-a"))))
        try {
            connection.connect()
            withTimeout(3_000) { while (connection.phase.value != AgentPhase.Live) delay(5) }
            // The same reducer the relay used to feed: the replayed narration is on screen.
            withTimeout(3_000) {
                while (connection.activity.value.feed.none { it is AgentFeedItem.Narration }) delay(5)
            }
            assertTrue(connection.sendMessage("over enet"))
            connection.interrupt()
            withTimeout(3_000) { while (node.sentTypes("dev-a").count { it != "join" } < 3) delay(5) }
            assertEquals(listOf("join", "input", "input", "interrupt"), node.sentTypes("dev-a"))
            assertEquals("over enet", node.sent[1].second["data"]!!.jsonPrimitive.content)
        } finally {
            connection.close()
        }
    }

    // ── VAPP-43: NO FEED, no relay fallback ─────────────────────────────

    @Test
    fun noFeedOnARunningRowIsStartingAndRedials() = runBlocking {
        val discoveries = AtomicInteger()
        val connection = connection(EnetSteerTransport(dialer(FakeNode(), emptyList(), discoveries)))
        try {
            connection.connect()
            withTimeout(3_000) { while (connection.phase.value != AgentPhase.Starting) delay(5) }
            // Redials on the Starting cadence, never anything else.
            withTimeout(3_000) { while (discoveries.get() < 3) delay(5) }
            assertEquals(AgentPhase.Starting, connection.phase.value)
            assertNull(connection.history.value)
            assertTrue(connection.activity.value.feed.isEmpty())
        } finally {
            connection.close()
        }
    }

    @Test
    fun noFeedOnAnEndedRowIsEndedWithTheDeviceOfflineLine() = runBlocking {
        // Every endpoint refused: no install runs it or holds its journal.
        val node = FakeNode().apply { serve(live = "nobody") }
        val discoveries = AtomicInteger()
        val connection = connection(
            EnetSteerTransport(dialer(node, listOf("dev-a"), discoveries)),
            status = DomainContract.sessionStatusEnded,
        )
        try {
            connection.connect()
            withTimeout(3_000) { while (connection.phase.value !is AgentPhase.Ended) delay(5) }
            assertEquals(AgentPhase.Ended(null), connection.phase.value)
            assertEquals(HistoryState.DeviceOffline, connection.history.value)
            // VAPP-169: nothing redials before staleMs; reapable once nobody
            // shows it, yet revivable.
            delay(200)
            assertEquals(1, discoveries.get())
            assertTrue(connection.isFinished)
            assertTrue(connection.isDeviceOffline)
            // A wakeup redials holding the phase; NO FEED again = the same copy.
            connection.kick("test")
            withTimeout(3_000) { while (discoveries.get() < 2) delay(5) }
            delay(50)
            assertEquals(AgentPhase.Ended(null), connection.phase.value)
            assertEquals(HistoryState.DeviceOffline, connection.history.value)
        } finally {
            connection.close()
        }
    }

    @Test
    fun anEndedRowsStoredReplayEndsWithItsTranscriptOnScreen() = runBlocking {
        // The device holds the journal: reset, replay, synced, then bye history.
        val node = FakeNode().apply {
            device = { peer, env ->
                if (env.types.contains("join")) {
                    deliver(peer, env.sessionId, *replay, """{"t":"bye","outcome":"history"}""")
                }
            }
        }
        val discoveries = AtomicInteger()
        val connection = connection(
            EnetSteerTransport(dialer(node, listOf("dev-a"), discoveries)),
            status = DomainContract.sessionStatusEnded,
        )
        try {
            connection.connect()
            withTimeout(3_000) {
                while (connection.activity.value.feed.none { it is AgentFeedItem.Narration }) delay(5)
            }
            withTimeout(3_000) { while (!connection.isFinished) delay(5) }
            assertEquals(AgentPhase.Ended(null), connection.phase.value)
            assertNull(connection.history.value)
            delay(200)
            assertEquals(1, discoveries.get())
        } finally {
            connection.close()
        }
    }

    /**
     * EXP-1531 through EXP-1529's carrier switch: the joined device's
     * `peerDisconnected` is an eNet DROP, so the relay opens at once (the
     * stall timer and staleMs are far beyond the test).
     */
    @Test
    fun aDisconnectOfTheJoinedPeerOpensTheRelayAtOnce() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val enet = EnetSteerTransport(
            EnetSteerDialer(
                node = { node },
                discover = { listOf("dev-a") },
                timings = fast.copy(staleMs = 60_000),
                dispatcher = Dispatchers.Default,
            ),
        )
        val relayDials = AtomicInteger()
        val relay = object : SteerTunnelDialer {
            override suspend fun open(sessionId: String): TunnelOpen {
                relayDials.incrementAndGet()
                return TunnelOpen.Unavailable("device_offline")
            }
        }
        val timings = SteerFallbackTimings(stallMs = 60_000, enetRedialMs = listOf(60_000), carrierWaitMs = 1_000)
        val socket = FallbackSteerTransport(enet, relay, timings, System::currentTimeMillis, Dispatchers.Default)
            .open(SESSION)!!
        try {
            repeat(3) { socket.next() }
            assertEquals(0, relayDials.get())
            check(node.drops.tryEmit("dev-a"))
            withTimeout(2_000) { while (relayDials.get() == 0) delay(5) }
            assertEquals(1, relayDials.get())
        } finally {
            socket.cancel()
        }
    }

    /** EXP-1531: the node's path kind to the joined device is connStats' `path`, through the carrier switch too. */
    @Test
    fun thePathKindOfTheJoinedDeviceIsTheFeedsPath() = runBlocking {
        val node = FakeNode().apply { serve(live = "dev-a") }
        val dialer = EnetSteerDialer(node = { node }, discover = { listOf("dev-a") }, timings = fast, dispatcher = Dispatchers.Default)
        val direct = dialer.open(SESSION)!!
        try {
            assertEquals("direct", direct.path)
            node.paths = mapOf("dev-a" to "iroh_relay")
            assertEquals("iroh_relay", direct.path)
        } finally {
            direct.cancel()
        }
        val carried = FallbackSteerTransport(EnetSteerTransport(dialer), null, dispatcher = Dispatchers.Default).open(SESSION)!!
        try {
            repeat(3) { carried.next() }
            assertEquals("iroh_relay", carried.path)
            node.paths = emptyMap()
            assertEquals("direct", carried.path)
        } finally {
            carried.cancel()
        }
    }

    private companion object {
        const val SESSION = "11111111-1111-4111-8111-111111111111"
    }
}
