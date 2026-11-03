package com.exponential.app.data.steer

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.DomainContract
import java.io.File
import java.time.Instant
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * EXP-1529: the sealed relay FALLBACK on Android. The fixture's
 * `carrierCases` and `connStats.validCases` replayed, the tunnel socket's
 * sealing and filters, the carrier switch over fake carriers (stall, no feed,
 * eNet killed mid-stream and back), and the analytics client.
 */
class SteerFallbackTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.steer.enetFixture")
            ?: error("exponential.steer.enetFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    // ── the fixture ─────────────────────────────────────────────────────

    @Test
    fun carrierCasesReplay() {
        val cases = fixture["carrierCases"]!!.jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        fun state(o: JsonObject) = SteerCarrierState(
            SteerActiveCarrier.entries.single { it.wire == o["active"]!!.jsonPrimitive.content },
            SteerRelayLeg.entries.single { it.wire == o["relay"]!!.jsonPrimitive.content },
        )
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val event = SteerCarrierEvent.entries.single { it.wire == case["event"]!!.jsonPrimitive.content }
            val step = steerCarrierStep(state(case["state"]!!.jsonObject), event)
            assertEquals(name, state(case["next"]!!.jsonObject), step.state)
            assertEquals(name, case["actions"]!!.jsonArray.map { it.jsonPrimitive.content }, step.actions.map { it.wire })
        }
    }

    @Test
    fun theFallbackConstantsAreTheFixtures() {
        val fallback = fixture["fallback"]!!.jsonObject
        assertEquals(fallback["stallMs"]!!.jsonPrimitive.long, STEER_FALLBACK_STALL_MS)
        assertEquals(fallback["tunnelMaxDataBytes"]!!.jsonPrimitive.int, STEER_TUNNEL_MAX_DATA_BYTES)
        assertEquals(fallback["relayRetryMs"]!!.jsonArray.map { it.jsonPrimitive.long }, STEER_RELAY_RETRY_MS)
        assertEquals(fallback["tunnelErrorCodes"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet(), STEER_TUNNEL_ERROR_CODES)
        // #EXP-1678: the starting backoff.
        val retry = fallback["startingRetry"]!!.jsonObject
        assertEquals(retry["baseMs"]!!.jsonPrimitive.long, STEER_STARTING_RETRY_BASE_MS)
        assertEquals(retry["maxMs"]!!.jsonPrimitive.long, STEER_STARTING_RETRY_MAX_MS)
        for (case in fixture["startingRetryCases"]!!.jsonArray.map { it.jsonObject }) {
            val n = case["retries"]!!.jsonPrimitive.int
            val cap = case["capMs"]!!.jsonPrimitive.long
            assertEquals("retries $n", cap, steerStartingRetryCapMs(n))
            assertEquals(cap / 2, steerStartingRetryDelayMs(n, random = 0.0))
            assertTrue(steerStartingRetryDelayMs(n, random = 0.999) < cap)
        }
        val stats = fixture["connStats"]!!.jsonObject
        fun set(key: String) = stats[key]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
        assertEquals(set("kinds"), SteerConnStatsSpec.kinds)
        assertEquals(set("sources"), SteerConnStatsSpec.sources)
        assertEquals(set("phases"), SteerConnStatsSpec.phases)
        assertEquals(set("platforms"), SteerConnStatsSpec.platforms)
        assertEquals(set("paths"), SteerConnStatsSpec.paths)
        assertEquals(set("outcomes"), SteerConnStatsSpec.outcomes)
        assertEquals(set("errorClasses"), SteerConnStatsSpec.errorClasses)
        assertEquals(stats["settleAfterMs"]!!.jsonPrimitive.long, SteerConnStatsSpec.SETTLE_AFTER_MS)
        assertEquals(stats["flushMs"]!!.jsonPrimitive.long, SteerConnStatsSpec.FLUSH_MS)
        assertEquals(stats["batchMax"]!!.jsonPrimitive.int, SteerConnStatsSpec.BATCH_MAX)
        assertEquals(stats["versionMax"]!!.jsonPrimitive.int, SteerConnStatsSpec.VERSION_MAX)
    }

    @Test
    fun connStatsValidCasesReplay() {
        val cases = fixture["connStats"]!!.jsonObject["validCases"]!!.jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            assertEquals(name, case["valid"]!!.jsonPrimitive.boolean, steerConnEventValid(case["event"]!!))
        }
    }

    /** #EXP-1678: a failed view always names its class. */
    @Test
    fun connStatsErrorClassCasesReplay() {
        val cases = fixture["connStats"]!!.jsonObject["errorClassCases"]!!.jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val expected = case["errorClass"]!!.jsonPrimitive.contentOrNull
            val got = steerViewErrorClass(
                ok = case["ok"]!!.jsonPrimitive.boolean,
                lastError = case["lastError"]!!.jsonPrimitive.contentOrNull,
                background = case["background"]!!.jsonPrimitive.boolean,
            )
            assertEquals(name, expected, got)
        }
    }

    @Test
    fun aViewWithNoFailureSettlesDialingAndParksBackground() {
        val events = mutableListOf<JsonObject>()
        var now = 0L
        var away = false
        val view = SteerViewStats({ events += it }, "1.2.3", { now }, background = { away })
        view.open()
        now = 10_000
        view.settleTimeout()
        away = true
        view.close()
        assertEquals("dialing", events[0]["data"]!!.jsonObject["errorClass"]!!.jsonPrimitive.content)
        assertEquals("background", events[1]["data"]!!.jsonObject["errorClass"]!!.jsonPrimitive.content)
        assertTrue(events.all(::steerConnEventValid))
    }

    // ── the tunnel ──────────────────────────────────────────────────────

    /** A stand-in for push_seal: reversible, but never the plaintext on the wire. */
    private class FakeSealer : SteerSealer {
        override fun seal(envelopeJson: String) = "v1:1:" + Base64.getUrlEncoder().encodeToString(envelopeJson.reversed().encodeToByteArray())
        override fun open(data: String): String? = runCatching {
            require(data.startsWith("v1:1:"))
            Base64.getUrlDecoder().decode(data.removePrefix("v1:1:")).decodeToString().reversed()
        }.getOrNull()
    }

    private class FakeLink : TunnelLink {
        val inbound = Channel<String>(Channel.UNLIMITED)
        val sent = CopyOnWriteArrayList<String>()
        @Volatile var closed = false
        @Volatile var onSend: FakeLink.(envelope: SteerEnvelope) -> Unit = {}
        private val sealer = FakeSealer()
        override val incoming: ReceiveChannel<String> get() = inbound
        override suspend fun send(text: String) {
            sent += text
            val data = Json.parseToJsonElement(text).jsonObject["data"]!!.jsonPrimitive.content
            parseSteerEnvelope(sealer.open(data)!!)?.let { onSend(it) }
        }
        override fun close() {
            closed = true
            inbound.close()
        }
        fun deliver(sessionId: String, vararg frames: String) {
            val body = encodeSteerEnvelope(sessionId, frames.map { Json.parseToJsonElement(it).jsonObject }).decodeToString()
            inbound.trySend(buildJsonObject { put("t", "sealed"); put("data", sealer.seal(body)) }.toString())
        }
    }

    private val fast = SteerEnetTimings(joinTimeoutMs = 400, staleMs = 5_000, sendTimeoutMs = 400)

    private fun tunnelSocket(link: FakeLink) =
        TunnelSteerSocket(link, SESSION, FakeSealer(), fast, System::currentTimeMillis, Dispatchers.Default)

    @Test
    fun theTunnelHoldsTheReplayAndSealsEveryFrame() = runBlocking {
        val link = FakeLink().apply {
            onSend = { env -> if (env.types.contains("join")) deliver(SESSION, *REPLAY) }
        }
        val socket = tunnelSocket(link)
        assertEquals(TunnelJoin.Joined, socket.probe())
        assertEquals(REPLAY.map { tag(it) }, List(3) { tag(socket.next()) })
        // Nothing on the wire names a frame: one `sealed` with ciphertext.
        val wire = Json.parseToJsonElement(link.sent.single()).jsonObject
        assertEquals("sealed", wire["t"]!!.jsonPrimitive.content)
        assertFalse(wire["data"]!!.jsonPrimitive.content.contains("join"))
        // The connection's own join is swallowed (the probe already joined); input goes out sealed.
        socket.send("""{"t":"join","channel":"activity"}""")
        socket.send("""{"t":"input","data":"hi"}""")
        assertEquals(2, link.sent.size)
        socket.cancel()
    }

    @Test
    fun theTunnelDropsWhatIsNotThisSessionsEnvelope() = runBlocking {
        val link = FakeLink().apply {
            onSend = { env ->
                if (env.types.contains("join")) {
                    deliver("another-session", """{"t":"activity_reset"}""")
                    inbound.trySend("""{"t":"sealed","data":"v1:1:not-base64!!"}""")
                    inbound.trySend("""{"t":"sealed","data":"plain text"}""")
                    deliver(SESSION, *REPLAY)
                }
            }
        }
        val socket = tunnelSocket(link)
        assertEquals(TunnelJoin.Joined, socket.probe())
        assertEquals("activity_reset", tag(socket.next()))
        socket.cancel()
    }

    @Test
    fun aTunnelErrorIsNoRelayLeg() = runBlocking {
        val link = FakeLink().apply {
            onSend = { inbound.trySend("""{"t":"tunnel_error","code":"device_unsupported"}"""); inbound.close() }
        }
        assertEquals(TunnelJoin.Unavailable("device_unsupported"), tunnelSocket(link).probe())
    }

    @Test
    fun theDialerNeedsAKeyAndATicket() = runBlocking {
        val keyless = RelayTunnelDialer({ null }, { error("never asked") }, { error("never dialled") }, fast)
        assertEquals(TunnelOpen.Unavailable("no_key"), keyless.open(SESSION))
        val disabled = RelayTunnelDialer({ FakeSealer() }, { TunnelTicket.Disabled }, { error("never dialled") }, fast)
        assertEquals(TunnelOpen.Unavailable(null), disabled.open(SESSION))
        val offline = RelayTunnelDialer({ FakeSealer() }, { TunnelTicket.Unavailable }, { error("never dialled") }, fast)
        assertEquals(TunnelOpen.Unavailable("device_offline"), offline.open(SESSION))
        val failing = RelayTunnelDialer({ FakeSealer() }, { error("500") }, { error("never dialled") }, fast)
        assertEquals(TunnelOpen.Failed("ticket_failed"), failing.open(SESSION))
        val urls = CopyOnWriteArrayList<String>()
        val granted = RelayTunnelDialer(
            { FakeSealer() },
            { TunnelTicket.Granted("t1", "wss://relay/tunnel?ticket=t1") },
            { url ->
                urls += url
                FakeLink().apply { onSend = { env -> if (env.types.contains("join")) deliver(SESSION, *REPLAY) } }
            },
            fast,
            dispatcher = Dispatchers.Default,
        )
        val joined = granted.open(SESSION)
        assertTrue(joined is TunnelOpen.Joined)
        assertEquals(listOf("wss://relay/tunnel?ticket=t1"), urls)
        (joined as TunnelOpen.Joined).socket.cancel()
    }

    @Test
    fun theTreeSealerRoundTripsWithTheRealSealedContent() {
        val key = SealedFfi.generateTreeKey()
        val sealer = TreeSteerSealer(SealedFfi, key.publicKey, 1) { mapOf(1 to key.secret) }
        val envelope = encodeSteerEnvelope(SESSION, listOf(buildJsonObject { put("t", "leave") })).decodeToString()
        val data = sealer.seal(envelope)
        assertTrue(data, data.startsWith("v1:1:"))
        assertFalse(data.contains("leave"))
        assertEquals(envelope, sealer.open(data))
        // Without the epoch's secret it does not open: the frame is dropped.
        assertNull(TreeSteerSealer(SealedFfi, key.publicKey, 1) { emptyMap() }.open(data))
        assertNull(sealer.open("v1:1:garbage"))
    }

    // ── the carrier switch ─────────────────────────────────────────────

    /** One carrier feed, as both legs hand it over (already joined, replay queued). */
    private class FakeFeed(val name: String) : SteerSocket {
        private val frames = Channel<String>(Channel.UNLIMITED)
        val sent = CopyOnWriteArrayList<String>()
        @Volatile var cancelled = false
        override val incoming: ReceiveChannel<String> get() = frames
        override suspend fun send(text: String) {
            sent += text
        }
        override fun cancel() {
            cancelled = true
            frames.close()
        }
        override suspend fun closeReason(): String? = if (cancelled) "cancelled" else "stale"
        fun emit(vararg texts: String) = texts.forEach { assertTrue(frames.trySend(it).isSuccess) }
        fun replay(text: String = "hi") = emit(
            """{"t":"activity_reset"}""",
            """{"t":"activity","seq":0,"event":{"kind":"narration","id":"n-$name","text":"$text"}}""",
            """{"t":"activity_synced","firstSeq":0,"lastSeq":0}""",
        )
        fun kill() = frames.close()
    }

    /** eNet: each dial runs [plan] (1-based); null = NO FEED. */
    private class FakeEnet : SteerTransport {
        val dials = AtomicInteger()
        val feeds = Channel<FakeFeed>(Channel.UNLIMITED)
        @Volatile var plan: suspend (Int) -> FakeFeed? = { FakeFeed("enet$it").apply { replay() } }
        override suspend fun open(codingSessionId: String): SteerSocket? {
            val feed = plan(dials.incrementAndGet()) ?: return null
            feeds.send(feed)
            return feed
        }
        override val lastFailure: String? get() = "no_endpoint"
    }

    private class FakeRelay : SteerTunnelDialer {
        val dials = AtomicInteger()
        val feeds = Channel<FakeFeed>(Channel.UNLIMITED)
        @Volatile var plan: suspend (Int) -> TunnelOpen = {
            TunnelOpen.Joined(FakeFeed("relay$it").apply { replay("from the relay") })
        }
        override suspend fun open(sessionId: String): TunnelOpen {
            val result = plan(dials.incrementAndGet())
            (result as? TunnelOpen.Joined)?.let { feeds.send(it.socket as FakeFeed) }
            return result
        }
    }

    private class Listener : SteerCarrierListener {
        val fallbacks = AtomicInteger()
        val failures = CopyOnWriteArrayList<String>()
        override fun carrierFallback() {
            fallbacks.incrementAndGet()
        }
        override fun carrierFailure(errorClass: String) {
            failures += errorClass
        }
    }

    private val quick = SteerFallbackTimings(stallMs = 150, enetRedialMs = listOf(100), carrierWaitMs = 1_000)

    private fun fallback(enet: FakeEnet, relay: SteerTunnelDialer?, timings: SteerFallbackTimings = quick) =
        FallbackSteerTransport(enet, relay, timings, System::currentTimeMillis, Dispatchers.Default)

    @Test
    fun eNetInTimeNeverTouchesTheRelay() = runBlocking {
        val relay = FakeRelay()
        val socket = fallback(FakeEnet(), relay).open(SESSION)!!
        assertEquals("activity_reset", tag(socket.next()))
        delay(300)
        assertEquals(0, relay.dials.get())
        assertEquals("direct", socket.path)
        socket.cancel()
    }

    @Test
    fun aStalledENetOpensTheRelayAndENetTakesOverLater() = runBlocking {
        val enet = FakeEnet()
        val enetGate = CompletableDeferred<Unit>()
        enet.plan = { dial ->
            if (dial == 1) enetGate.await()
            FakeFeed("enet$dial").apply { replay("from eNet") }
        }
        val relay = FakeRelay()
        val transport = fallback(enet, relay)
        val listener = Listener().also { transport.listener = it }
        val startedAt = System.currentTimeMillis()
        val socket = transport.open(SESSION)!!
        assertTrue(System.currentTimeMillis() - startedAt >= 140)
        assertEquals(1, relay.dials.get())
        assertEquals(1, listener.fallbacks.get())
        assertEquals("server_fallback", socket.path)
        assertEquals(listOf("activity_reset", "activity", "activity_synced"), List(3) { tag(socket.next()) })
        val relayFeed = relay.feeds.receive()
        // eNet arrives: its reset takes over, the relay closes once eNet delivers.
        enetGate.complete(Unit)
        assertEquals("activity_reset", tag(socket.next()))
        assertTrue(socket.next().contains("from eNet"))
        waitUntil("the relay closed") { relayFeed.cancelled }
        assertEquals("direct", socket.path)
        // Relay frames after the takeover are dropped.
        socket.send("""{"t":"join","channel":"activity"}""")
        socket.send("""{"t":"input","data":"x"}""")
        val enetFeed = enet.feeds.receive()
        assertEquals(listOf("""{"t":"input","data":"x"}"""), enetFeed.sent.toList())
        socket.cancel()
    }

    @Test
    fun noENetFeedOpensTheRelayAtOnce() = runBlocking {
        val enet = FakeEnet().apply { plan = { dial -> if (dial == 1) null else { delay(10_000); null } } }
        val relay = FakeRelay()
        val transport = fallback(enet, relay, quick.copy(stallMs = 10_000))
        val listener = Listener().also { transport.listener = it }
        val startedAt = System.currentTimeMillis()
        val socket = transport.open(SESSION)!!
        assertTrue(System.currentTimeMillis() - startedAt < 5_000)
        assertEquals("server_fallback", socket.path)
        assertTrue(listener.failures.contains("no_endpoint"))
        socket.cancel()
    }

    @Test
    fun bothCarriersDownIsNoFeed() = runBlocking {
        val enet = FakeEnet().apply { plan = { null } }
        val unavailable = fallback(enet, FakeRelay().apply { plan = { TunnelOpen.Unavailable("device_offline") } })
        assertNull(unavailable.open(SESSION))
        val noRelay = fallback(FakeEnet().apply { plan = { null } }, null)
        assertNull(noRelay.open(SESSION))
        // The relay failing at the stall, then eNet no feed: NO FEED, no second relay dial.
        val relay = FakeRelay().apply { plan = { TunnelOpen.Failed("join_timeout") } }
        val slow = FakeEnet().apply { plan = { delay(400); null } }
        assertNull(fallback(slow, relay).open(SESSION))
        assertEquals(1, relay.dials.get())
    }

    @Test
    fun anENetDropWithoutARelayLegEndsTheFeedLikeBefore() = runBlocking {
        val enet = FakeEnet()
        val socket = fallback(enet, null).open(SESSION)!!
        repeat(3) { socket.next() }
        enet.feeds.receive().kill()
        assertNull(withTimeout(2_000) { socket.incoming.receiveCatching().getOrNull() })
        assertNotNull(socket.downSinceMs())
    }

    @Test
    fun aWakeupRedialsENetAtOnce() = runBlocking {
        val enet = FakeEnet().apply { plan = { dial -> if (dial == 1) null else FakeFeed("enet$dial").apply { replay() } } }
        val relay = FakeRelay()
        // A background redial far beyond the test's patience: only the wakeup redials.
        val socket = fallback(enet, relay, quick.copy(enetRedialMs = listOf(60_000))).open(SESSION)!!
        assertEquals("server_fallback", socket.path)
        repeat(3) { socket.next() }
        socket.wake()
        assertEquals("activity_reset", tag(socket.next()))
        assertEquals(2, enet.dials.get())
        waitUntil("eNet carries") { socket.path == "direct" }
        socket.cancel()
    }

    // ── kill eNet mid-stream: the view keeps streaming ────────────────

    private fun row() = SessionEntity(
        id = SESSION,
        teamId = "team-1",
        userId = "user-1",
        status = DomainContract.sessionStatusRunning,
        startedAt = Instant.now().toString(),
        createdAt = Instant.now().toString(),
        updatedAt = Instant.now().toString(),
    )

    @Test
    fun killingENetMidStreamKeepsTheViewStreamingOverTheRelayAndBack() = runBlocking {
        val enet = FakeEnet()
        val relay = FakeRelay()
        val transport = fallback(enet, relay, quick.copy(enetRedialMs = listOf(300)))
        val events = CopyOnWriteArrayList<JsonObject>()
        val connection = SteerConnection(
            codingSessionId = SESSION,
            transport = transport,
            sessionFlow = MutableStateFlow(row()),
            json = Json { ignoreUnknownKeys = true },
            dispatcher = Dispatchers.Unconfined,
            nowMs = { System.currentTimeMillis() },
            timings = SteerTimings(startingRetryMs = 50, reconnectBaseMs = 50, reconnectMaxMs = 50, joinAckMs = 2_000),
            stats = SteerConnStatsSink { events += it },
            version = "0.0.1-test",
        )
        val phases = CopyOnWriteArrayList<AgentPhase>()
        val watcher = launch(Dispatchers.Default) { connection.phase.collect { phases += it } }
        try {
            // Hold eNet's background redial back until the relay has carried a while.
            val enetBack = CompletableDeferred<Unit>()
            enet.plan = { dial ->
                if (dial > 1) enetBack.await()
                FakeFeed("enet$dial").apply { replay("eNet $dial") }
            }
            connection.connect()
            waitUntil("live over eNet") { connection.phase.value == AgentPhase.Live && narration(connection).contains("eNet 1") }
            val first = enet.feeds.receive()
            phases.clear()

            first.kill()
            waitUntil("the relay carries") { narration(connection).contains("from the relay") }
            val relayFeed = relay.feeds.receive()
            relayFeed.emit("""{"t":"activity","seq":1,"event":{"kind":"narration","id":"n-live","text":"live over the relay"}}""")
            waitUntil("live relay frames") { narration(connection).contains("live over the relay") }
            // Steering rides the relay meanwhile.
            assertTrue(connection.sendMessage("steer me"))
            waitUntil("input on the relay") { relayFeed.sent.any { it.contains("steer me") } }

            enetBack.complete(Unit)
            waitUntil("back on eNet") { narration(connection).contains("eNet 2") }
            waitUntil("the relay closed") { relayFeed.cancelled }
            // Invisible: the phase never left Live, the composer never lost its socket for long.
            assertTrue(phases.toString(), phases.all { it == AgentPhase.Live })
            assertEquals(AgentPhase.Live, connection.phase.value)
            assertEquals(1, relay.dials.get())
            assertTrue(connection.connected.value)
        } finally {
            connection.close()
            watcher.cancel()
        }
        // The view's analytics: one ok settle over eNet, then the ended view with its fallback.
        assertEquals(listOf("settled", "ended"), events.map { it["data"]!!.jsonObject["phase"]!!.jsonPrimitive.content })
        val settled = events[0]["data"]!!.jsonObject
        assertEquals("direct", settled["path"]!!.jsonPrimitive.content)
        assertEquals("android", settled["platform"]!!.jsonPrimitive.content)
        val ended = events[1]["data"]!!.jsonObject
        assertEquals(1, ended["fallbacks"]!!.jsonPrimitive.int)
        assertTrue(ended["failures"]!!.jsonObject.containsKey("stale"))
        assertTrue(events.all(::steerConnEventValid))
        assertFalse(events.any { it.toString().contains(SESSION) })
    }

    // ── the background: the OS cut the feed, nothing falls back ────────

    /**
     * Prod, 2026-11-01 (Android 0.14.89): 13 of 19 views ended with
     * `{closed: 1, no_endpoint: 2-3, ticket_failed: 3-4}` and as many
     * fallbacks, all of a phone's views in the same millisecond: the feed
     * died while the app sat frozen in the background, and every redial and
     * relay ticket of the fallback failed on the frozen process. A feed that
     * goes down in the background now parks: no relay dial, no eNet redial,
     * no failure on the view; the foreground redials it.
     */
    @Test
    fun aFeedThatDropsInTheBackgroundParksWithoutAFallback() = runBlocking {
        val enet = FakeEnet()
        val relay = FakeRelay()
        val transport = fallback(enet, relay, quick.copy(enetRedialMs = listOf(50)))
        val events = CopyOnWriteArrayList<JsonObject>()
        val connection = SteerConnection(
            codingSessionId = SESSION,
            transport = transport,
            sessionFlow = MutableStateFlow(row()),
            json = Json { ignoreUnknownKeys = true },
            dispatcher = Dispatchers.Unconfined,
            nowMs = { System.currentTimeMillis() },
            timings = SteerTimings(startingRetryMs = 50, reconnectBaseMs = 50, reconnectMaxMs = 50, joinAckMs = 2_000),
            stats = SteerConnStatsSink { events += it },
            version = "0.0.1-test",
        )
        try {
            connection.connect()
            waitUntil("live over eNet") { connection.phase.value == AgentPhase.Live && narration(connection).contains("hi") }
            val first = enet.feeds.receive()

            connection.setBackground(true)
            first.kill()
            waitUntil("parked") { connection.phase.value == AgentPhase.Idle }
            delay(300)
            assertEquals("no relay leg in the background", 0, relay.dials.get())
            assertEquals("no eNet redial in the background", 1, enet.dials.get())
            assertEquals(listOf("settled", "ended"), events.map { it["data"]!!.jsonObject["phase"]!!.jsonPrimitive.content })
            val ended = events[1]["data"]!!.jsonObject
            assertEquals(0, ended["fallbacks"]!!.jsonPrimitive.int)
            assertNull(ended["failures"])
            assertNull(ended["errorClass"])

            // The foreground redials: a fresh view, eNet first, live again.
            connection.setBackground(false)
            connection.resume()
            waitUntil("live again") { connection.phase.value == AgentPhase.Live && enet.dials.get() == 2 }
            assertEquals(0, relay.dials.get())
        } finally {
            connection.close()
        }
        assertTrue(events.all(::steerConnEventValid))
    }

    @Test
    fun aPausedTransportOpensNoRelayOnAStall() = runBlocking {
        val enet = FakeEnet().apply { plan = { null } }
        val relay = FakeRelay()
        val transport = fallback(enet, relay).apply { paused = true }
        val listener = Listener().also { transport.listener = it }
        assertNull(transport.open(SESSION))
        assertEquals(0, relay.dials.get())
        assertEquals(0, listener.fallbacks.get())
        assertTrue(listener.failures.isEmpty())
    }

    // ── analytics ───────────────────────────────────────────────────────

    @Test
    fun aViewThatNeverGetsAFrameSettlesFailed() {
        val events = mutableListOf<JsonObject>()
        var now = 0L
        val view = SteerViewStats({ events += it }, "1.2.3", { now })
        view.open()
        view.carrierFailure("no_endpoint")
        view.carrierFailure("device_offline")
        now = 10_000
        view.settleTimeout()
        view.close()
        assertEquals(2, events.size)
        val settled = events[0]["data"]!!.jsonObject
        assertEquals("failed", settled["outcome"]!!.jsonPrimitive.content)
        assertEquals("device_offline", settled["errorClass"]!!.jsonPrimitive.content)
        assertNull(settled["path"])
        assertTrue(events.all(::steerConnEventValid))
    }

    @Test
    fun theStatsClientBatchesAndRetries() = runBlocking {
        val posts = CopyOnWriteArrayList<Int>()
        val failNext = AtomicInteger(1)
        val client = SteerConnStatsClient(
            post = { batch ->
                if (failNext.getAndDecrement() > 0) error("offline")
                posts += batch.size
            },
            flushMs = 60_000,
        )
        repeat(120) { i -> client.record(buildJsonObject { put("n", i) }) }
        assertTrue(client.flushNow())
        assertEquals(120, client.buffered())
        assertFalse(client.flushNow())
        assertEquals(listOf(50, 50, 20), posts.toList())
        assertEquals(0, client.buffered())
    }

    // ── helpers ─────────────────────────────────────────────────────────

    private fun narration(connection: SteerConnection) =
        connection.activity.value.feed.filterIsInstance<AgentFeedItem.Narration>().joinToString { it.text }

    private suspend fun SteerSocket.next(): String = withTimeout(3_000) { incoming.receive() }

    private fun tag(text: String) = Json.parseToJsonElement(text).jsonObject["t"]!!.jsonPrimitive.content

    private suspend fun waitUntil(what: String, timeoutMs: Long = 5_000, check: () -> Boolean) {
        val ok = withTimeoutOrNull(timeoutMs) {
            while (!check()) delay(5)
            true
        }
        if (ok != true) fail("timed out waiting for $what")
    }

    private companion object {
        const val SESSION = "11111111-1111-4111-8111-111111111111"
        val REPLAY = arrayOf(
            """{"t":"activity_reset"}""",
            """{"t":"activity","seq":0,"event":{"kind":"narration","id":"n1","text":"hi"}}""",
            """{"t":"activity_synced","firstSeq":0,"lastSeq":0}""",
        )
    }
}
