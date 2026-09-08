package com.exponential.app.data.steer

import com.exponential.app.data.api.SteerTicketResult
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.DomainContract
import java.time.Instant
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * Dial-loop tests for [SteerConnection] (EXP-625).
 *
 * The bug: background the app, come back two minutes later, and the session
 * screen sat on "Connecting…" forever: the dial coroutine had died silently
 * under a phase no revival path would touch, so re-opening the screen and
 * backgrounding again both no-oped. Everything here is one of the ways it
 * died, or one of the ways it now comes back.
 *
 * The socket and the ticket mint ride the [SteerTransport] seam, so the loop
 * is exercised with no engine and no clock to wait on.
 */
class SteerConnectionTest {

    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    /** Millisecond-scale so a failing regression takes a second, not a minute. */
    private val fastTimings = SteerTimings(
        startingRetryMs = 50,
        reconnectBaseMs = 50,
        reconnectMaxMs = 50,
        joinAckMs = 300,
        upgradeMs = 2_000,
        liveStaleMs = 200,
    )

    private class FakeSocket : SteerSocket {
        private val frames = Channel<String>(Channel.UNLIMITED)
        val sent = CopyOnWriteArrayList<String>()

        @Volatile var cancelled = false

        @Volatile var code: Int? = null

        @Volatile var reason: String? = null

        /** Makes the next [send] throw the ktor-shaped foreign cancellation. */
        @Volatile var failSend: (() -> Throwable)? = null

        override val incoming: ReceiveChannel<String> get() = frames

        override suspend fun send(text: String) {
            failSend?.let { throw it() }
            sent += text
        }

        override fun cancel() {
            cancelled = true
            frames.close()
        }

        override suspend fun closeCode(): Int? = code

        override suspend fun closeReason(): String? = reason

        /** Push a relay frame at the viewer. */
        fun emit(text: String) {
            assertTrue(frames.trySend(text).isSuccess)
        }

        /** The relay hangs up on us. */
        fun hangUp(withCode: Int? = null) {
            code = withCode
            frames.close()
        }
    }

    private class FakeTransport : SteerTransport {
        val opens = Channel<FakeSocket>(Channel.UNLIMITED)
        private val dials = AtomicInteger(0)

        /** Shape the Nth socket (1-based) before the dial gets hold of it. */
        @Volatile var prepare: (Int, FakeSocket) -> Unit = { _, _ -> }

        override suspend fun mint(codingSessionId: String) =
            SteerTicketResult(ticket = "tkt", url = "wss://relay.test/ws?ticket=tkt")

        override suspend fun open(url: String): SteerSocket {
            val socket = FakeSocket()
            prepare(dials.incrementAndGet(), socket)
            opens.send(socket)
            return socket
        }

        suspend fun awaitOpen(): FakeSocket = withTimeout(5_000) { opens.receive() }
    }

    private fun runningRow() = CodingSessionEntity(
        id = SESSION_ID,
        teamId = "team-1",
        userId = "user-1",
        status = DomainContract.codingSessionStatusRunning,
        startedAt = Instant.now().toString(),
        createdAt = Instant.now().toString(),
        updatedAt = Instant.now().toString(),
    )

    private fun connection(
        transport: FakeTransport,
        timings: SteerTimings = fastTimings,
        row: MutableStateFlow<CodingSessionEntity?> = MutableStateFlow(null),
    ): SteerConnection {
        if (row.value == null) row.value = runningRow()
        return SteerConnection(
            codingSessionId = SESSION_ID,
            transport = transport,
            sessionFlow = row,
            json = json,
            dispatcher = Dispatchers.Unconfined,
            nowMs = { System.currentTimeMillis() },
            timings = timings,
        )
    }

    private suspend fun waitUntil(what: String, timeoutMs: Long = 5_000, check: () -> Boolean) {
        val settled = withTimeoutOrNull(timeoutMs) {
            while (!check()) delay(2)
            true
        }
        if (settled != true) fail("timed out waiting for $what")
    }

    @Test
    fun anAnsweredJoinTakesTheConnectionLive() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport)
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            assertEquals(listOf(JOIN_FRAME), socket.sent.toList())
            // The relay answers every join with activity_reset + its replay.
            socket.emit("""{"t":"activity_reset"}""")
            waitUntil("the live phase") { connection.phase.value == AgentPhase.Live }
            assertTrue(connection.connected.value)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aKickWakesTheStartingRetryAtOnce() = runBlocking {
        val transport = FakeTransport()
        // A retry cadence far longer than this test's patience: only the kick
        // can produce the second dial in time.
        val connection = connection(transport, fastTimings.copy(startingRetryMs = 30_000))
        try {
            connection.connect()
            val first = transport.awaitOpen()
            first.emit("""{"t":"error","code":"no_such_session"}""")
            waitUntil("the starting phase") { connection.phase.value == AgentPhase.Starting }
            connection.kick("test")
            assertNotSame(first, transport.awaitOpen())
        } finally {
            connection.close()
        }
    }

    @Test
    fun aForeignCancellationDropsTheDialAndNotTheLoop() = runBlocking {
        val transport = FakeTransport()
        transport.prepare = { dial, socket ->
            // ktor's OkHttp engine closes `outgoing` with a
            // java.util.concurrent.CancellationException when the relay hangs
            // up first, the same class our own cancellation uses. Rethrowing
            // it ended the dial coroutine with no log and no phase write, and
            // nothing could revive the connection afterwards.
            if (dial == 1) {
                socket.failSend = {
                    java.util.concurrent.CancellationException("Outgoing channel was closed")
                }
            }
        }
        // Slow enough that the reconnecting phase is observable before the
        // redial flips it back to Connecting.
        val connection = connection(
            transport,
            fastTimings.copy(reconnectBaseMs = 400, reconnectMaxMs = 400),
        )
        try {
            connection.connect()
            transport.awaitOpen()
            waitUntil("a reconnecting phase") {
                connection.phase.value.let { it is AgentPhase.Closed && it.reconnecting }
            }
            assertEquals(listOf(JOIN_FRAME), transport.awaitOpen().sent.toList())
        } finally {
            connection.close()
        }
    }

    @Test
    fun aMuteSocketIsDroppedOnTheJoinAckDeadline() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(
            transport,
            fastTimings.copy(joinAckMs = 100, reconnectBaseMs = 400, reconnectMaxMs = 400),
        )
        try {
            connection.connect()
            // A socket that upgrades and then says nothing: ktor's HttpTimeout
            // does not cover this, and the frame loop used to wait forever.
            val first = transport.awaitOpen()
            waitUntil("a reconnecting phase") {
                connection.phase.value.let { it is AgentPhase.Closed && it.reconnecting }
            }
            assertTrue(first.cancelled)
            assertFalse(connection.connected.value)
            assertNotSame(first, transport.awaitOpen())
        } finally {
            connection.close()
        }
    }

    @Test
    fun parkGoesIdleAndAKickRedials() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport)
        try {
            connection.connect()
            val first = transport.awaitOpen()
            connection.park()
            assertEquals(AgentPhase.Idle, connection.phase.value)
            assertTrue(first.cancelled)
            assertFalse(connection.connected.value)

            connection.kick("test")
            val second = transport.awaitOpen()
            second.emit("""{"t":"activity_reset"}""")
            waitUntil("the live phase") { connection.phase.value == AgentPhase.Live }
            assertTrue(connection.connected.value)

            // The superseded socket unwinding late must not take the live one's
            // state with it: the dial's cleanup used to null `ws` and flip
            // `connected` false whether or not it still owned them.
            first.hangUp(1006)
            delay(50)
            assertEquals(AgentPhase.Live, connection.phase.value)
            assertTrue(connection.connected.value)
            assertFalse(second.cancelled)
        } finally {
            connection.close()
        }
    }

    // EXP-648: the relay ticks a keepalive to every joined viewer, so a quiet
    // agent (parked on a question or a plan approval) no longer reads as a
    // dead socket, and a kick does not throw a healthy socket away for a
    // mint + activity_reset + full replay.
    @Test
    fun aKeepaliveFrameKeepsALiveSocketFresh() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport)
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            waitUntil("the live phase") { connection.phase.value == AgentPhase.Live }
            // Well past liveStaleMs in total, never past it between beats.
            repeat(4) {
                delay(fastTimings.liveStaleMs / 2)
                socket.emit("""{"t":"keepalive"}""")
            }
            connection.kick("foreground")
            assertNull(withTimeoutOrNull(300) { transport.opens.receive() })
            assertEquals(AgentPhase.Live, connection.phase.value)
            assertTrue(connection.connected.value)
            assertFalse(socket.cancelled)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aMuteLiveSocketIsRedialedSilentlyOnKick() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport)
        try {
            connection.connect()
            val first = transport.awaitOpen()
            first.emit("""{"t":"activity_reset"}""")
            waitUntil("the live phase") { connection.phase.value == AgentPhase.Live }

            delay(fastTimings.liveStaleMs + 100)
            connection.kick("foreground")
            val second = transport.awaitOpen()
            // A probe of a socket that may turn out fine must not flash
            // "Reconnecting…": the phase holds Live across the redial.
            assertEquals(AgentPhase.Live, connection.phase.value)
            assertTrue(first.cancelled)

            second.emit("""{"t":"activity_reset"}""")
            waitUntil("the redial to connect") { connection.connected.value }
            assertEquals(AgentPhase.Live, connection.phase.value)
        } finally {
            connection.close()
        }
    }

    // ── Staged replay (EXP-656) ─────────────────────────────────────────────
    //
    // The relay answers every join with activity_reset + a full replay of the
    // room log. Applying that literally emptied the feed for the length of the
    // burst, which disposed the LazyColumn (and with it the reader's scroll
    // position, the follow flag and the focused subagent tab) and dumped
    // someone reading a plan at the bottom of the replay.

    /** Millisecond-scale staging windows, so a failing case fails fast. */
    private val stagingTimings = fastTimings.copy(replayQuietMs = 40, replayMaxMs = 200)

    private fun narration(text: String) =
        """{"t":"activity","event":{"kind":"narration","text":"$text"}}"""

    /** A live connection with an established feed — the state a reader parked
     *  mid-plan is in when the relay decides to replay at them. */
    private suspend fun liveWithFeed(
        transport: FakeTransport,
        connection: SteerConnection,
    ): FakeSocket {
        connection.connect()
        val socket = transport.awaitOpen()
        socket.emit("""{"t":"activity_reset"}""")
        socket.emit(narration("original one"))
        socket.emit("""{"t":"activity_synced"}""")
        waitUntil("the first feed") { connection.activity.value.feed.size == 1 }
        return socket
    }

    @Test
    fun aResetAndReplayCommitsOnceAndNeverShowsAnEmptyFeed() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            val sizes = CopyOnWriteArrayList<Int>()
            val watcher = launch(Dispatchers.Unconfined) {
                connection.activity.collect { sizes += it.feed.size }
            }

            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed one"))
            socket.emit(narration("replayed two"))
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the committed replay") { connection.activity.value.feed.size == 2 }
            delay(stagingTimings.replayQuietMs * 3)
            watcher.cancel()

            // One emission for the whole burst, and never an empty list in
            // between (which is what remounted the feed and lost the anchor).
            assertEquals(listOf(1, 2), sizes.toList())
            // The prefix keeps its ids, so the LazyColumn keys still line up.
            assertEquals(listOf(0L, 1L), connection.activity.value.feed.map { it.id })
        } finally {
            connection.close()
        }
    }

    @Test
    fun aReplayWithNoEndMarkerCommitsOnTheQuietTimeout() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            // An old relay: reset + replay, no activity_synced.
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed one"))
            // The old feed holds until the burst goes quiet.
            assertEquals(1, connection.activity.value.feed.size)
            waitUntil("the quiet commit") {
                connection.activity.value.feed.singleOrNull()?.let {
                    it is com.exponential.app.domain.AgentFeedItem.Narration &&
                        it.text == "replayed one"
                } == true
            }
        } finally {
            connection.close()
        }
    }

    @Test
    fun eventsArrivingDuringStagingLandInTheCommittedFeed() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed one"))
            // A genuinely new event racing the tail of the replay is
            // indistinguishable on the wire — it must not be dropped.
            socket.emit(narration("live during replay"))
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("both events") { connection.activity.value.feed.size == 2 }
            assertEquals(
                listOf("replayed one", "live during replay"),
                connection.activity.value.feed.map {
                    (it as com.exponential.app.domain.AgentFeedItem.Narration).text
                },
            )
        } finally {
            connection.close()
        }
    }

    @Test
    fun aKeepaliveEndsAStagedReplay() = runBlocking {
        val transport = FakeTransport()
        // No quiet/cap rescue in this window: only the keepalive can commit.
        val connection = connection(
            transport,
            fastTimings.copy(replayQuietMs = 30_000, replayMaxMs = 30_000),
        )
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed one"))
            // The relay's own 15s beat proves the burst is over.
            socket.emit("""{"t":"keepalive"}""")
            waitUntil("the keepalive commit") {
                connection.activity.value.feed.singleOrNull()?.let {
                    it is com.exponential.app.domain.AgentFeedItem.Narration &&
                        it.text == "replayed one"
                } == true
            }
        } finally {
            connection.close()
        }
    }

    @Test
    fun aNeverQuietReplayCommitsAtTheHardCap() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(
            transport,
            // Quiet can never fire: something arrives every few ms.
            fastTimings.copy(replayQuietMs = 30_000, replayMaxMs = 150),
        )
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            val pump = launch {
                var n = 0
                while (isActive) {
                    socket.emit(narration("replayed ${n++}"))
                    delay(5)
                }
            }
            waitUntil("the capped commit") { connection.activity.value.feed.size > 1 }
            pump.cancel()
            // …and the stream keeps appending normally afterwards.
            val committed = connection.activity.value.feed.size
            socket.emit(narration("after the cap"))
            waitUntil("a post-commit append") {
                connection.activity.value.feed.size > committed
            }
        } finally {
            connection.close()
        }
    }

    @Test
    fun aSocketCloseDuringStagingKeepsTheVisibleFeed() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings.copy(replayMaxMs = 30_000))
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            socket.hangUp(1006)
            waitUntil("the dial to unwind") { !connection.connected.value }
            delay(stagingTimings.replayQuietMs * 3)
            // A half-delivered replay is worth less than the last complete
            // picture — the reader keeps what they were reading.
            assertEquals(1, connection.activity.value.feed.size)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aSecondResetRestartsStaging() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings.copy(replayMaxMs = 30_000))
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("abandoned"))
            // The publisher republished mid-replay: the first buffer is dead.
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("restarted"))
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the restarted replay") {
                connection.activity.value.feed.singleOrNull()?.let {
                    it is com.exponential.app.domain.AgentFeedItem.Narration &&
                        it.text == "restarted"
                } == true
            }
        } finally {
            connection.close()
        }
    }

    @Test
    fun anAnswerSentDuringStagingKeepsItsLock() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings.copy(replayMaxMs = 30_000))
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(QUESTION_FRAME)
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the question card") { connection.activity.value.feed.size == 1 }

            // The replay window is ≤400ms in production — a plan-approval tap
            // lands inside it more often than one would like, and its card must
            // not come back unlocked (a double-tap would re-answer the ask).
            socket.emit("""{"t":"activity_reset"}""")
            connection.sendQuestionAnswer("q1", askId = null, keys = listOf("1"))
            socket.emit(QUESTION_FRAME)
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the recommitted card") { connection.activity.value.feed.size == 1 }
            assertEquals(
                com.exponential.app.domain.AnswerState.Sending,
                connection.activity.value.answerLocks["q1"],
            )
        } finally {
            connection.close()
        }
    }

    @Test
    fun aMessageSentDuringStagingSurvivesTheCommit() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings.copy(replayMaxMs = 30_000))
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit("""{"t":"activity_reset"}""")
            // The replay predates this message, so only the local record of it
            // can put it back.
            assertTrue(connection.sendMessage("steered mid-replay"))
            socket.emit(narration("replayed one"))
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the committed replay") { connection.activity.value.feed.size == 2 }
            val texts = connection.activity.value.feed.map {
                when (it) {
                    is com.exponential.app.domain.AgentFeedItem.Narration -> it.text
                    is com.exponential.app.domain.AgentFeedItem.UserMessage -> it.text
                    else -> ""
                }
            }
            assertEquals(listOf("replayed one", "steered mid-replay"), texts)
        } finally {
            connection.close()
        }
    }

    @Test
    fun anUnknownFrameIsIgnored() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            // The protocol only ever grows; a frame from a newer relay must
            // change nothing at all.
            socket.emit("""{"t":"something_new","payload":{"a":1}}""")
            socket.emit("""not json at all""")
            delay(stagingTimings.replayQuietMs * 3)
            assertEquals(1, connection.activity.value.feed.size)
            assertEquals(AgentPhase.Live, connection.phase.value)
            assertTrue(connection.connected.value)
        } finally {
            connection.close()
        }
    }

    @Test
    fun theCompactionStripNeverOutlivesTheSession() = runBlocking {
        // EXP-724: `compaction started` pins an indeterminate strip. Its
        // `ended` normally takes it down — but a run that ENDS under it never
        // sends one, and a strip stuck up forever reads as a hung agent.
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit(
                """{"t":"activity","event":{"kind":"compaction","phase":"started","trigger":"manual"}}""",
            )
            waitUntil("the compaction strip") { connection.activity.value.compacting != null }
            assertEquals("manual", connection.activity.value.compacting?.trigger)
            // The strip is state, not a row: the feed is untouched.
            assertEquals(1, connection.activity.value.feed.size)

            socket.emit("""{"t":"bye","outcome":"ended"}""")
            // The relay hangs up right behind its bye.
            socket.hangUp()
            waitUntil("the ended phase") { connection.phase.value is AgentPhase.Ended }
            assertNull(connection.activity.value.compacting)
        } finally {
            connection.close()
        }
    }

    // ── EXP-746: steering v2 control frames ─────────────────────────────────

    @Test
    fun setConfigSendsOneSetConfigFrame() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            val before = socket.sent.size
            connection.setConfig("model", "opus")
            waitUntil("the set_config frame") { socket.sent.size > before }
            // Fire-and-forget: one frame, no ack, no lock.
            assertEquals(
                json.parseToJsonElement("""{"t":"set_config","id":"model","value":"opus"}"""),
                json.parseToJsonElement(socket.sent.last()),
            )
            assertTrue(connection.activity.value.answerLocks.isEmpty())
            // A blank value is the "CLI default" pick and still goes out.
            connection.setConfig("effort", "")
            waitUntil("the blank set_config frame") { socket.sent.size > before + 1 }
            assertEquals(
                json.parseToJsonElement("""{"t":"set_config","id":"effort","value":""}"""),
                json.parseToJsonElement(socket.sent.last()),
            )
        } finally {
            connection.close()
        }
    }

    @Test
    fun setModeSendsOneSetModeFrame() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            val before = socket.sent.size
            connection.setMode("plan")
            waitUntil("the set_mode frame") { socket.sent.size > before }
            assertEquals(
                json.parseToJsonElement("""{"t":"set_mode","id":"plan"}"""),
                json.parseToJsonElement(socket.sent.last()),
            )
        } finally {
            connection.close()
        }
    }

    @Test
    fun aSetConfigOnADeadSocketSendsNothing() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            val before = socket.sent.size
            connection.park()
            connection.setConfig("model", "opus")
            connection.setMode("plan")
            // Nothing to send on, and nothing thrown either.
            delay(stagingTimings.replayQuietMs * 3)
            assertEquals(before, socket.sent.size)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aStagedReplayReDerivesConfigAndUsageOnCommit() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings.copy(replayMaxMs = 30_000))
        try {
            val socket = liveWithFeed(transport, connection)
            socket.emit(CONFIG_FRAME)
            socket.emit(USAGE_FRAME)
            waitUntil("the live config") { connection.activity.value.config != null }

            // Every viewer join triggers a replay: the commit folds from a
            // FRESH state, so both slots must come back off the replayed
            // snapshots or the chips blank out on every reconnect.
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed one"))
            socket.emit(CONFIG_FRAME)
            socket.emit(USAGE_FRAME)
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the committed replay") {
                connection.activity.value.feed.any {
                    it is com.exponential.app.domain.AgentFeedItem.Narration &&
                        it.text == "replayed one"
                }
            }
            assertEquals("opus", connection.activity.value.config?.options?.first()?.value)
            assertEquals(200_000, connection.activity.value.usage?.contextSize)

            // A replay that carries neither leaves the slots empty — they are
            // state derived from the log, not a sticky client cache.
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(narration("replayed two"))
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the second commit") {
                connection.activity.value.feed.any {
                    it is com.exponential.app.domain.AgentFeedItem.Narration &&
                        it.text == "replayed two"
                }
            }
            assertNull(connection.activity.value.config)
            assertNull(connection.activity.value.usage)
        } finally {
            connection.close()
        }
    }

    @Test
    fun closeIsFinalAndAKickCannotReviveIt() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport)
        connection.connect()
        transport.awaitOpen()
        connection.close()
        // Not left on Connecting behind a cancelled scope: a ViewModel still
        // holding this connection would render that spinner forever.
        assertEquals(AgentPhase.Closed(reconnecting = false), connection.phase.value)
        connection.kick("test")
        assertNull(withTimeoutOrNull(300) { transport.opens.receive() })
    }

    // ── EXP-773: the end banner never prints a protocol word ─────────────────

    @Test
    fun protocolOutcomesCarryNoBannerCaption() {
        // Viewing an ENDED run ends with the journal republish closing itself
        // out — the banner used to print that outcome verbatim, so it read
        // "history". The two failures already have their own HistoryState line.
        for (outcome in listOf("ended", "history", "history_unavailable", "device_offline")) {
            assertNull(outcome, steerEndDetail(outcome))
        }
        assertNull(steerEndDetail(null))
        assertNull(steerEndDetail(""))
        // Anything the relay coins that isn't a known protocol word still shows.
        assertEquals("publisher_lost", steerEndDetail("publisher_lost"))
        assertEquals("killed", steerEndDetail("killed"))
        // Cross-client contract: iOS `SteerOutcome.silentEndOutcomes`, web
        // `steer-session-store.ts`.
        assertEquals(
            setOf("ended", "history", "history_unavailable", "device_offline"),
            SILENT_END_OUTCOMES,
        )
    }

    @Test
    fun aHistoryByeEndsWithThePlainCaption() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            // The relay's journal republish signs off once the transcript is in.
            socket.emit("""{"t":"bye","outcome":"history"}""")
            socket.hangUp()
            waitUntil("the ended phase") { connection.phase.value is AgentPhase.Ended }
            assertEquals(AgentPhase.Ended(null), connection.phase.value)
            // The transcript it delivered stays on screen.
            assertEquals(1, connection.activity.value.feed.size)
        } finally {
            connection.close()
        }
    }

    // ── EXP-788: the composer answers a pending card ─────────────────────

    @Test
    fun aDraftAnswersThePendingPlanCardInsteadOfStartingATurn() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(PLAN_FRAME)
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the plan card") { connection.activity.value.feed.size == 1 }

            connection.setDraft("Make the migration reversible")
            connection.sendDraft()
            waitUntil("the answer frame") { socket.sent.any { it.contains(""""t":"answer"""") } }
            val answer = socket.sent.single { it.contains(""""t":"answer"""") }
            // The reject option (the LAST one since EXP-788) carries the text.
            assertTrue(answer, answer.contains(""""questionId":"plan1""""))
            assertTrue(answer, answer.contains(""""keys":["3"]"""))
            assertTrue(answer, answer.contains(""""text":"Make the migration reversible""""))
            // No `input` frame: it was an answer, not a new turn.
            assertFalse(socket.sent.any { it.contains(""""t":"input"""") })
            // The card locked and the draft went with the frame.
            assertEquals(
                com.exponential.app.domain.AnswerState.Sending,
                connection.activity.value.answerLocks["plan1"],
            )
            assertEquals("", connection.draft.value)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aDraftWithNoPendingCardIsAPlainSteerMessage() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            val socket = liveWithFeed(transport, connection)
            connection.setDraft("carry on")
            connection.sendDraft()
            waitUntil("the input frame") { socket.sent.any { it.contains(""""t":"input"""") } }
            assertFalse(socket.sent.any { it.contains(""""t":"answer"""") })
            assertEquals("", connection.draft.value)
        } finally {
            connection.close()
        }
    }

    @Test
    fun aDraftOverALockedCardStaysPutUntilTheCardReopens() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit(PLAN_FRAME)
            socket.emit("""{"t":"activity_synced"}""")
            waitUntil("the plan card") { connection.activity.value.feed.size == 1 }
            connection.sendQuestionAnswer("plan1", askId = null, keys = listOf("1"))
            // Locked: the typed text is a plain message now (the card is
            // answered as far as this client knows), never a second answer.
            connection.setDraft("and also this")
            connection.sendDraft()
            waitUntil("the input frame") { socket.sent.any { it.contains(""""t":"input"""") } }
            assertEquals(1, socket.sent.count { it.contains(""""t":"answer"""") })
        } finally {
            connection.close()
        }
    }

    // ── EXP-796: earlier pages need an open socket ───────────────────────

    @Test
    fun canLoadEarlierNeedsATruncatedReplayAndAnOpenSocket() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            socket.emit("""{"t":"activity","seq":40,"event":{"kind":"narration","text":"tail"}}""")
            socket.emit("""{"t":"activity_synced","truncated":true,"firstSeq":40}""")
            waitUntil("the truncated tail") { connection.activity.value.feed.size == 1 }
            // The relay keeps the room open after the replay: pages flow.
            assertTrue(connection.canLoadEarlier())
            assertTrue(connection.loadEarlier())
            waitUntil("the page ask") { socket.sent.any { it.contains(""""t":"history_page"""") } }
            // The device signed off and the socket really closed: nothing
            // is left to ask, whatever the replay said.
            socket.emit("""{"t":"bye","outcome":"history"}""")
            socket.hangUp()
            waitUntil("the ended phase") { connection.phase.value is AgentPhase.Ended }
            assertFalse(connection.canLoadEarlier())
            assertFalse(connection.loadEarlier())
        } finally {
            connection.close()
        }
    }

    @Test
    fun anUntruncatedReplayNeverOffersEarlierPages() = runBlocking {
        val transport = FakeTransport()
        val connection = connection(transport, stagingTimings)
        try {
            liveWithFeed(transport, connection)
            assertFalse(connection.canLoadEarlier())
        } finally {
            connection.close()
        }
    }
}

private const val SESSION_ID = "11111111-2222-3333-4444-555555555555"
private const val JOIN_FRAME = """{"t":"join","channel":"activity"}"""

/** One semantic question card (EXP-249 wire id `q1`) — the plan-approval
 *  shape whose lock has to survive a replay. */
/** EXP-746: a `config_state` snapshot — latest-wins state, replayed on join. */
private const val CONFIG_FRAME =
    """{"t":"activity","event":{"kind":"config_state","options":[""" +
        """{"id":"model","label":"Model","value":"opus",""" +
        """"values":[{"id":"opus","label":"Opus"}]}],"currentMode":"plan",""" +
        """"modes":[{"id":"plan","label":"Plan"}]}}"""

/** EXP-746: the run's context meter, same latest-wins rule. */
private const val USAGE_FRAME =
    """{"t":"activity","event":{"kind":"usage","contextUsed":124000,""" +
        """"contextSize":200000,"costUsd":1.24}}"""

private const val QUESTION_FRAME =
    """{"t":"activity","event":{"kind":"question","id":"q1","text":"Approve?",""" +
        """"options":[{"label":"Yes","key":"1"},{"label":"No","key":"2"}]}}"""

/** EXP-788: a plan-approval card as the desktop publishes it since EXP-788 —
 *  the plain "Yes" first, the reject LAST with its description. */
private const val PLAN_FRAME =
    """{"t":"activity","event":{"kind":"question","id":"plan1","planMode":true,"text":"## Plan",""" +
        """"options":[{"label":"Yes","key":"1"},""" +
        """{"label":"Yes, and start with a fresh context","key":"2"},""" +
        """{"label":"No, keep planning","key":"3","description":"Sends your next message back to planning"}]}}"""
