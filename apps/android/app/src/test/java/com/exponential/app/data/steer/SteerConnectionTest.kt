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
}

private const val SESSION_ID = "11111111-2222-3333-4444-555555555555"
private const val JOIN_FRAME = """{"t":"join","channel":"activity"}"""
