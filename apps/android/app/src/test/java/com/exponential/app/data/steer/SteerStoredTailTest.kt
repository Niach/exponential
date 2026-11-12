package com.exponential.app.data.steer

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.sealed.OpenedBody
import com.exponential.app.data.sealed.OpenedCard
import com.exponential.app.data.sealed.SealedCrypto
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.sealed.TreeKey
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.TailEvent
import com.exponential.app.domain.adoptRowIds
import com.exponential.app.domain.foldStoredTail
import java.io.File
import java.nio.file.Files
import java.time.Instant
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * #PERF-9: fixtures/steer-enet.json STORED TAIL. The tail a viewer keeps per
 * opened session: sealed for a keyed tree (never in clear), as is for a
 * keyless one, capped at `storedTail`, replaced by every join's replay, and
 * painted on open until that replay replaces it.
 */
class SteerStoredTailTest {
    private val dir: File = Files.createTempDirectory("steer-tail").toFile()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @After
    fun tearDown() {
        scope.cancel()
        dir.deleteRecursively()
    }

    private fun narration(i: Int): JsonObject = buildJsonObject {
        put("kind", "narration")
        put("text", "reply number $i, with a secret word")
        put("messageId", "m$i")
    }

    private fun events(range: IntRange) = range.map { TailEvent(narration(it), it.toLong()) }

    @Test
    fun theCapIsTheFixtures() {
        val path = System.getProperty("exponential.steer.enetFixture") ?: error("exponential.steer.enetFixture is not set")
        val fixture = Json.parseToJsonElement(File(path).readText()).jsonObject
        assertEquals(fixture.getValue("storedTail").jsonPrimitive.int, STEER_STORED_TAIL)
    }

    @Test
    fun aKeyedTreesTailRoundTripsSealed() {
        val key = SealedFfi.generateTreeKey()
        val tree = TailTree("root-1", key.publicKey, 3)
        val clear = StoredTailCodec.encode("s-1", events(1..5))
        val stored = StoredTailCodec.seal(clear, tree, SealedFfi)
        assertNotNull(stored)
        assertTrue(stored!!.startsWith("exp-sealed:v1:3:"))
        assertFalse("the tail is never on disk in clear", stored.contains("secret word"))

        val opened = StoredTailCodec.open(stored, tree, mapOf(3 to key.secret), SealedFfi)
        assertEquals(clear, opened)
        val decoded = StoredTailCodec.decode(opened!!, "s-1")!!
        assertEquals((1L..5L).toList(), decoded.map { it.seq })
        assertEquals(narration(4), decoded[3].event)
        // Another session's tail is not this one's.
        assertNull(StoredTailCodec.decode(opened, "s-2"))
    }

    @Test
    fun aTailThatDoesNotOpenIsNotShown() {
        val key = SealedFfi.generateTreeKey()
        val tree = TailTree("root-1", key.publicKey, 1)
        val stored = StoredTailCodec.seal(StoredTailCodec.encode("s-1", events(1..2)), tree, SealedFfi)!!
        // No key here yet.
        assertNull(StoredTailCodec.open(stored, tree, emptyMap(), SealedFfi))
        // Another tree's key.
        val other = SealedFfi.generateTreeKey()
        assertNull(StoredTailCodec.open(stored, tree, mapOf(1 to other.secret), SealedFfi))
        // Malformed.
        assertNull(StoredTailCodec.open("exp-sealed:v1:1:garbage", tree, mapOf(1 to key.secret), SealedFfi))
        assertNull(StoredTailCodec.decode("{not json", "s-1"))
    }

    @Test
    fun aKeyedTreeIsNeverStoredOrShownInClear() {
        val clear = StoredTailCodec.encode("s-1", events(1..2))
        // A seal that fails, or hands back anything but an envelope, writes nothing.
        val failing = FakeCrypto(seal = { error("no seal") })
        val echoing = FakeCrypto(seal = { it })
        val tree = TailTree("root-1", "pk", 1)
        assertNull(StoredTailCodec.seal(clear, tree, failing))
        assertNull(StoredTailCodec.seal(clear, tree, echoing))
        // A clear file on a keyed tree does not open.
        assertNull(StoredTailCodec.open(clear, tree, emptyMap(), SealedFfi))
        // A keyless tree's tail is stored as is.
        val keyless = TailTree("root-1", null, 1)
        assertEquals(clear, StoredTailCodec.seal(clear, keyless, failing))
        assertEquals(clear, StoredTailCodec.open(clear, keyless, emptyMap(), failing))
    }

    @Test
    fun theRecorderKeepsTheNewest200() = runBlocking {
        val writes = mutableListOf<List<TailEvent>>()
        val recorder = SteerTailRecorder(scope, persist = { writes += it }, debounceMs = 60_000)
        events(1..250).forEach(recorder::append)
        assertEquals((51L..250L).toList(), recorder.snapshot().map { it.seq })
        recorder.replace(events(1000..1299))
        assertEquals((1100L..1299L).toList(), recorder.snapshot().map { it.seq })
        recorder.replace(events(1..3))
        assertEquals(listOf(1L, 2L, 3L), recorder.snapshot().map { it.seq })
        // Debounced: nothing written yet; a flush writes the latest once.
        assertTrue(writes.isEmpty())
        recorder.flush()
        waitUntil("the flush") { writes.size == 1 }
        assertEquals(listOf(1L, 2L, 3L), writes.single().map { it.seq })
    }

    @Test
    fun aLatestWinsSlotKeepsOnlyItsNewestEvent() {
        val recorder = SteerTailRecorder(scope, persist = {}, debounceMs = 60_000)
        events(1..150).forEach(recorder::append)
        // A working agent's `turn` every couple of seconds never pushes rows out.
        repeat(500) { i ->
            recorder.append(TailEvent(buildJsonObject { put("kind", "turn"); put("state", "started"); put("tokens", i) }, null))
        }
        val kept = recorder.snapshot()
        assertEquals(151, kept.size)
        assertEquals((1L..150L).toList(), kept.dropLast(1).map { it.seq })
        assertEquals(JsonPrimitive(499), kept.last().event["tokens"])
    }

    @Test
    fun signingOutDeletesTheTailsAndDropsALateWrite() {
        val files = SteerTailFiles(dir)
        val generation = files.generation("acct")
        assertTrue(files.write("acct", "s-1", "tail", generation))
        assertTrue(files.write("other", "s-1", "theirs", files.generation("other")))
        assertEquals("tail", files.read("acct", "s-1"))
        files.forget("acct")
        assertNull(files.read("acct", "s-1"))
        // A write already on its way for the signed-out account lands nowhere.
        assertFalse(files.write("acct", "s-1", "late", generation))
        assertNull(files.read("acct", "s-1"))
        assertEquals("theirs", files.read("other", "s-1"))
        // Signed in again: tails are kept again.
        assertTrue(files.write("acct", "s-1", "again", files.generation("acct")))
    }

    @Test
    fun theReplayReplacesThePaintedTailAndBecomesTheTail() = runBlocking {
        val writes = mutableListOf<List<TailEvent>>()
        val recorder = SteerTailRecorder(scope, persist = { writes += it }, debounceMs = 60_000)
        val transport = OneSocketTransport()
        val connection = SteerConnection(
            codingSessionId = "s-1",
            transport = transport,
            sessionFlow = MutableStateFlow(runningRow()),
            json = Json { ignoreUnknownKeys = true },
            dispatcher = Dispatchers.Unconfined,
            nowMs = { System.currentTimeMillis() },
            tail = recorder,
        )
        try {
            // The stored tail (seqs 300..309) paints at once.
            val painted = foldStoredTail(events(300..309))
            connection.seedStoredTail(painted)
            assertEquals((300L..309L).toList(), connection.activity.value.feed.map { it.seq })
            val paintedIds = connection.activity.value.feed.associate { it.seq to it.id }

            connection.connect()
            val socket = transport.awaitOpen()
            socket.emit("""{"t":"activity_reset"}""")
            // The replay (seqs 1..310): older rows, the tail's, one newer.
            for (i in 1..310) {
                socket.emit("""{"t":"activity","seq":$i,"event":${narration(i)}}""")
            }
            // Staging: the painted tail stays on screen, no placeholder frame.
            assertEquals(10, connection.activity.value.feed.size)
            socket.emit("""{"t":"activity_synced","firstSeq":1,"lastSeq":310}""")
            waitUntil("the replay commit") { connection.activity.value.feed.size == 310 }

            val feed = connection.activity.value.feed
            assertEquals((1L..310L).toList(), feed.map { it.seq })
            // The rows both held keep the tail's ids: the reader's anchor holds.
            for (seq in 300L..309L) assertEquals(paintedIds[seq], feed.first { it.seq == seq }.id)
            assertEquals(feed.map { it.id }.sorted(), feed.map { it.id })
            assertEquals(feed.size, feed.map { it.id }.toSet().size)
            // The replay is the new tail: its newest 200.
            assertEquals((111L..310L).toList(), recorder.snapshot().map { it.seq })

            // A live event appends; a later reset + replay replaces it all.
            socket.emit("""{"t":"activity","seq":311,"event":${narration(311)}}""")
            assertEquals(311L, recorder.snapshot().last().seq)
            socket.emit("""{"t":"activity_reset"}""")
            for (i in 400..402) socket.emit("""{"t":"activity","seq":$i,"event":${narration(i)}}""")
            socket.emit("""{"t":"activity_synced","firstSeq":400,"lastSeq":402}""")
            waitUntil("the second commit") { connection.activity.value.feed.lastOrNull()?.seq == 402L }
            assertEquals(listOf(400L, 401L, 402L), recorder.snapshot().map { it.seq })
        } finally {
            connection.close()
        }
    }

    @Test
    fun aTailIsNeverPaintedOverAFeed() {
        val connection = SteerConnection(
            codingSessionId = "s-1",
            transport = OneSocketTransport(),
            sessionFlow = MutableStateFlow(runningRow()),
            json = Json,
            dispatcher = Dispatchers.Unconfined,
            nowMs = { 0L },
        )
        try {
            connection.seedStoredTail(foldStoredTail(events(1..3)))
            assertEquals(3, connection.activity.value.feed.size)
            connection.seedStoredTail(foldStoredTail(events(10..20)))
            assertEquals(3, connection.activity.value.feed.size)
        } finally {
            connection.close()
        }
    }

    @Test
    fun adoptedIdsBailOutWhenTheyCannotHold() {
        val tail = foldStoredTail(events(5..7)).feed // ids 0, 1, 2
        // The replay holds a row BETWEEN two adjacent ids of the tail's: no room for it.
        val gapped = foldStoredTail(
            listOf(TailEvent(narration(5), 5), TailEvent(narration(50), 50), TailEvent(narration(6), 6)),
        )
        val adopted = gapped.adoptRowIds(tail)
        assertEquals("no room between ids 0 and 1: the replay keeps its own", gapped, adopted)
        // Older rows go below, newer ones above.
        val wider = foldStoredTail(events(1..9)).adoptRowIds(tail)
        assertEquals((-4L..4L).toList(), wider.feed.map { it.id })
        assertEquals(5L, wider.nextEventId)
        assertTrue(wider.feed.filter { it.seq in 5L..7L }.map { it.id } == listOf(0L, 1L, 2L))
        assertTrue(wider.feed.all { it is AgentFeedItem.Narration })
    }

    // ── fakes ─────────────────────────────────────────────────────────────

    private class FakeCrypto(private val seal: (String) -> String) : SealedCrypto {
        override fun generateTreeKey(): TreeKey = error("unused")
        override fun wrapTreeKey(installPublic: String, treeSecret: ByteArray): String = error("unused")
        override fun unwrapTreeKey(installSecret: ByteArray, wrapped: String): ByteArray = error("unused")
        override fun sealBody(treePublicKey: String, epoch: Int, text: String): String = seal(text)
        override fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody = error("unused")
        override fun sealCard(treePublicKey: String, epoch: Int, cardJson: String): String = error("unused")
        override fun openCard(cardJson: String, keys: Map<Int, ByteArray>): OpenedCard = error("unused")
        override fun installSecret(identity: ByteArray): ByteArray = error("unused")
    }

    private class FakeSocket : SteerSocket {
        private val frames = Channel<String>(Channel.UNLIMITED)
        override val incoming: ReceiveChannel<String> get() = frames
        override suspend fun send(text: String) = Unit
        override fun cancel() {
            frames.close()
        }
        override suspend fun closeReason(): String? = null
        fun emit(text: String) {
            assertTrue(frames.trySend(text).isSuccess)
        }
    }

    private class OneSocketTransport : SteerTransport {
        private val opens = Channel<FakeSocket>(Channel.UNLIMITED)
        override suspend fun open(codingSessionId: String): SteerSocket = FakeSocket().also { opens.send(it) }
        suspend fun awaitOpen(): FakeSocket = withTimeout(5_000) { opens.receive() }
    }

    private fun runningRow() = SessionEntity(
        id = "s-1",
        teamId = "team-1",
        userId = "user-1",
        status = DomainContract.sessionStatusRunning,
        startedAt = Instant.now().toString(),
        createdAt = Instant.now().toString(),
        updatedAt = Instant.now().toString(),
    )

    private suspend fun waitUntil(what: String, check: () -> Boolean) {
        val settled = withTimeoutOrNull(5_000) {
            while (!check()) delay(2)
            true
        }
        if (settled != true) fail("timed out waiting for $what")
    }
}
