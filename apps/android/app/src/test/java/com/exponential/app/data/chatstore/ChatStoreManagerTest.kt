package com.exponential.app.data.chatstore

import at.exponential.chatstore.ffi.CatchUp
import at.exponential.chatstore.ffi.CatchUpInstall
import at.exponential.chatstore.ffi.CatchUpSession
import at.exponential.chatstore.ffi.ChatStore
import at.exponential.chatstore.ffi.PageAccess
import at.exponential.chatstore.ffi.phoneRetentionDays
import com.exponential.app.data.api.TeamInstall
import com.exponential.app.data.db.ArrivedMessage
import com.exponential.app.data.db.CatchUpRow
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.PeerChat
import com.exponential.app.data.enet.PeerNode
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.steer.EnetJoin
import com.exponential.app.data.steer.EnetSteerSocket
import com.exponential.app.data.steer.SteerEnetTimings
import com.exponential.app.data.steer.SteerTailFiles
import com.exponential.app.data.steer.StoredTailCodec
import com.exponential.app.data.steer.TailTree
import com.exponential.app.data.steer.encodeSteerEnvelope
import com.exponential.app.data.steer.parseSteerEnvelope
import com.exponential.app.domain.TailEvent
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
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
 * #EDE-114 (chat-store.json S3) on Android, against the facade's host build
 * (`cargo build -p chat-store-ffi --release`, loaded by JNA from
 * `jna.library.path`): the store's place and lifetime (open, expiry keeps,
 * a deliberate sign-out deletes), OPEN from the store, the old tail's
 * migration, the storage setting, the catch-up answers' routing and the
 * holder's answers.
 */
class ChatStoreManagerTest {
    private val dir: File = Files.createTempDirectory("chat-store").toFile()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val now = 1_800_000_000_000L
    private val day = 86_400_000L

    @After
    fun tearDown() {
        scope.cancel()
        dir.deleteRecursively()
    }

    // ── fakes ─────────────────────────────────────────────────────────────

    /** The Room `messages` table as SQLite keeps it: `INSERT OR REPLACE`
     *  deletes a conflicting row and inserts it at the next `rowid`
     *  (max + 1, so an emptied table counts from 1 again). */
    private class MessageTable {
        val rows = mutableListOf<ArrivedMessage>()

        operator fun plusAssign(row: MessageEntity) {
            synchronized(rows) {
                rows.removeAll { it.row.id == row.id }
                rows += ArrivedMessage((rows.maxOfOrNull { it.arrival } ?: 0L) + 1, row)
            }
        }

        fun clear() = synchronized(rows) { rows.clear() }

        fun snapshot(): List<ArrivedMessage> = synchronized(rows) { rows.toList() }
    }

    private class Rows : ChatStoreRows {
        val sessions = mutableMapOf<String, SessionEntity>()
        val messages = MessageTable()
        /** Every page the store asked for: (from, limit). */
        val pages = mutableListOf<Pair<Long, Int>>()
        val messagesStampFlow = MutableStateFlow("0")

        override suspend fun session(accountId: String, sessionId: String) = sessions[sessionId]
        override suspend fun catchUpRows(accountId: String) = sessions.values.filter { it.deviceId.isNotEmpty() }
            .map { CatchUpRow(it.id, it.rootId, it.deviceId, it.updatedAt, it.busy) }
        override suspend fun messagesFrom(accountId: String, from: Long, limit: Int): List<ArrivedMessage> {
            synchronized(pages) { pages += from to limit }
            return messages.snapshot().filter { it.arrival >= from && !it.row.sessionId.isNullOrEmpty() }
                .sortedBy { it.arrival }.take(limit)
        }
        override suspend fun highestArrival(accountId: String) = messages.snapshot().maxOfOrNull { it.arrival }
        override suspend fun messagesByIds(accountId: String, ids: List<String>) =
            messages.snapshot().map { it.row }.filter { it.id in ids }
        override fun messagesStamp(accountId: String): Flow<String?> = messagesStampFlow
        override fun sessionsStamp(accountId: String): Flow<String?> = MutableStateFlow("0")
    }

    private class Net : ChatStoreNet {
        override val ready = MutableStateFlow<Set<String>>(emptySet())
        val registries = mutableMapOf<String, List<TeamInstall>>()
        var node: PeerNode? = null
        override fun peer(accountId: String) = node
        override fun registry(accountId: String) = registries
        override fun userId(accountId: String) = "u1"
    }

    private class Prefs : KeepToolStepsPrefs {
        var value: Int? = null
        override fun read() = value
        override fun write(value: Int) {
            this.value = value
        }
    }

    /** A node whose sends are recorded and whose chats the test emits. */
    private class FakeNode(override val endpointId: String = "me") : PeerNode {
        val sent = mutableListOf<Pair<String, String>>()
        val incoming = MutableSharedFlow<PeerChat>()
        override fun allowedPeers(): Set<String> = emptySet()
        override fun allow(peer: String) = Unit
        override fun revoke(peer: String) = Unit
        override suspend fun sendChat(peer: String, body: ByteArray) {
            synchronized(sent) { sent += peer to body.decodeToString() }
        }
        override val chats: SharedFlow<PeerChat> = incoming
        override suspend fun shutdown() = Unit
    }

    private val rows = Rows()
    private val net = Net()
    private val prefs = Prefs()
    private val settings = ChatStoreSettings(prefs)
    private val signedIn = MutableStateFlow<String?>("acct1")
    private val keys = MutableStateFlow(0)
    private val lockedIds = mutableSetOf<String>()
    private val tails = SteerTailFiles(File(dir, "steer-tail"))
    private var treeKeys: Map<Int, ByteArray> = emptyMap()

    private fun manager() = ChatStoreManager(
        dataDir = File(dir, "chat-store"),
        tailFiles = tails,
        signedIn = signedIn,
        rows = rows,
        net = net,
        settings = settings,
        unmetered = MutableStateFlow(false),
        messageBody = { row -> if (row.id in lockedIds) null else row.body },
        openOldTail = { stored, tree -> StoredTailCodec.open(stored, tree, treeKeys, SealedFfi) },
        keys = keys,
        scope = scope,
        nowMs = { now },
        timings = ChatStoreTimings(writeDebounceMs = 20, messagesDebounceMs = 20, keysDebounceMs = 20, openWaitMs = 5_000, wipeTries = 2),
    )

    private fun session(id: String, deviceId: String = "d1", rootId: String = id, userId: String = "u1", shared: Boolean = false, sealKey: String? = null) =
        SessionEntity(
            id = id,
            teamId = "t1",
            rootId = rootId,
            userId = userId,
            deviceId = deviceId,
            shared = shared,
            status = "running",
            createdAt = "2026-11-17T10:00:00.000Z",
            updatedAt = "2026-11-17T10:00:00.000Z",
            sealPublicKey = sealKey,
            sealEpoch = sealKey?.let { 1 },
        ).also { rows.sessions[id] = it }

    private fun narration(i: Int): JsonObject = buildJsonObject {
        put("kind", "narration")
        put("text", "reply $i")
        put("at", now)
    }

    private fun tool(i: Int, at: Long): JsonObject = buildJsonObject {
        put("kind", "tool")
        put("name", "Bash")
        put("id", "t$i")
        put("at", at)
    }

    private fun storeFile(account: String) = File(dir, "chat-store/accounts/$account/chat-store.sqlite")

    private suspend fun waitUntil(what: String, check: suspend () -> Boolean) {
        val ok = withTimeoutOrNull(10_000) {
            while (!check()) delay(10)
            true
        }
        if (ok != true) fail("timed out waiting for $what")
    }

    // ── tests ─────────────────────────────────────────────────────────────

    @Test
    fun theDefaultWindowIsTheFacadesPhoneRetention() {
        assertEquals(phoneRetentionDays().toInt(), KeepToolSteps.DEFAULT_DAYS)
    }

    @Test
    fun aSessionOpensFromTheStoreThroughTheReplayTail() = runBlocking {
        session("s1")
        val stores = manager()
        stores.start()
        waitUntil("the store open") { stores.openAccountId == "acct1" }
        assertTrue(storeFile("acct1").isFile)
        assertNull("nothing stored yet", stores.openTail("s1"))

        // A view's feed (replay + live), events without a seq are not storable.
        val feed = stores.feedStore("s1")
        feed.record((1..600).map { TailEvent(narration(it), it.toLong()) })
        feed.record(listOf(TailEvent(narration(0), null)))
        stores.flushFeeds()
        val tail = stores.openTail("s1")!!
        assertEquals((101L..600L).toList(), tail.events.map { it.seq })
        assertTrue("older events exist below the replay tail", tail.truncated)
        // Older pages come from the store.
        val older = feed.older(101, 200)!!
        assertEquals((1L..100L).toList(), older.map { it.seq })
        // Another device's session has none of them.
        session("s1", deviceId = "d2")
        assertNull(stores.openTail("s1"))
    }

    @Test
    fun anOldTailIsReadInOnceThenDeleted() = runBlocking {
        // A keyless tree's tail, and a keyed one whose key is not here yet.
        session("s1")
        val key = SealedFfi.generateTreeKey()
        session("s2", sealKey = key.publicKey)
        val events1 = (1..3).map { TailEvent(narration(it), it.toLong()) }
        val events2 = (7..9).map { TailEvent(narration(it), it.toLong()) }
        tails.write("acct1", "s1", StoredTailCodec.encode("s1", events1), tails.generation("acct1"))
        val sealed = StoredTailCodec.seal(StoredTailCodec.encode("s2", events2), TailTree("s2", key.publicKey, 1), SealedFfi)!!
        tails.write("acct1", "s2", sealed, tails.generation("acct1"))

        val stores = manager()
        stores.start()
        waitUntil("the keyless tail migrated") { tails.read("acct1", "s1") == null }
        assertEquals(listOf(1L, 2L, 3L), stores.openTail("s1")!!.events.map { it.seq })
        // No key: it stays (sealed, never read in clear) until the key arrives.
        assertNull(stores.openTail("s2"))
        assertNotNull(tails.read("acct1", "s2"))
        treeKeys = mapOf(1 to key.secret)
        keys.value += 1
        waitUntil("the sealed tail migrated") { tails.read("acct1", "s2") == null }
        assertEquals(listOf(7L, 8L, 9L), stores.openTail("s2")!!.events.map { it.seq })
    }

    @Test
    fun aSessionExpiryKeepsTheStoreAndADeliberateSignOutDeletesIt() = runBlocking {
        session("s1")
        val stores = manager()
        stores.start()
        waitUntil("the store open") { stores.openAccountId == "acct1" }
        stores.feedStore("s1").record(listOf(TailEvent(narration(1), 1)))
        stores.flushFeeds()
        // Another account's store is never touched.
        ChatStore.open(File(dir, "chat-store").path, "acct10").close()

        // The token is lost (expiry): closed, every file kept.
        signedIn.value = null
        waitUntil("the store closed") { stores.openAccountId == null }
        assertTrue(storeFile("acct1").isFile)
        // Signed in again: the same store, the same events.
        signedIn.value = "acct1"
        waitUntil("the store reopened") { stores.openAccountId == "acct1" }
        assertEquals(listOf(1L), stores.openTail("s1")!!.events.map { it.seq })

        // A deliberate sign-out: stopped, closed, deleted (the -wal/-shm too).
        tails.write("acct1", "s9", "old tail", tails.generation("acct1"))
        stores.signOut("acct1").join()
        assertNull(stores.openAccountId)
        val left = File(dir, "chat-store/accounts/acct1").listFiles().orEmpty().map { it.name }
        assertTrue("left: $left", left.none { it.startsWith("chat-store.sqlite") })
        assertNull(tails.read("acct1", "s9"))
        assertTrue(storeFile("acct10").isFile)
        // Still signed in a moment longer: it does not reopen.
        delay(100)
        assertNull(stores.openAccountId)
        assertFalse(storeFile("acct1").exists())
        // The credentials gone, then a new sign-in: a fresh store.
        signedIn.value = null
        delay(50)
        signedIn.value = "acct1"
        waitUntil("a fresh store") { stores.openAccountId == "acct1" }
        assertNull(stores.openTail("s1"))
    }

    @Test
    fun theStorageSettingAppliesAtOnce() = runBlocking {
        assertEquals(30, settings.days.value)
        session("s1")
        val stores = manager()
        stores.start()
        waitUntil("the store open") { stores.openAccountId == "acct1" }
        val feed = stores.feedStore("s1")
        // Prose 40 days old, a tool step 10 days old, one from today.
        feed.record(
            listOf(
                TailEvent(buildJsonObject { put("kind", "user_message"); put("text", "old"); put("at", now - 40 * day) }, 1),
                TailEvent(tool(2, now - 10 * day), 2),
                TailEvent(tool(3, now), 3),
            ),
        )
        stores.flushFeeds()
        val store = stores.openStoreForTests!!
        assertEquals(3uL, store.count("s1", "d1"))
        settings.setDays(7)
        assertEquals(7, prefs.value)
        waitUntil("the shorter window applied") { store.count("s1", "d1") == 2uL }
        assertEquals(listOf(1L, 3L), stores.openTail("s1")!!.events.map { it.seq })
        // Forever = -1 on disk, null in the store.
        settings.setDays(null)
        assertEquals(-1, prefs.value)
        assertNull(KeepToolSteps.decode(prefs.value))
    }

    @Test
    fun theSettingsOptionsAndTheirStoredForm() {
        assertEquals(listOf("7 days", "30 days", "90 days", "1 year", "Forever"), KeepToolSteps.OPTIONS.map { it.label })
        assertEquals(listOf(7, 30, 90, 365, null), KeepToolSteps.OPTIONS.map { it.days })
        assertEquals("Keep tool steps", KeepToolSteps.LABEL)
        assertEquals(30, KeepToolSteps.decode(null))
        assertEquals(90, KeepToolSteps.decode(90))
        assertNull(KeepToolSteps.decode(-1))
        // A value no option has falls back to the default.
        assertEquals(30, KeepToolSteps.decode(12))
        assertEquals(null, KeepToolSteps.fromId("forever")!!.days)
        assertEquals(365, KeepToolSteps.fromId("365")!!.days)
    }

    @Test
    fun syncedMessagesAreStoredAndASealedOneOpensWhenItsKeyArrives() = runBlocking {
        session("s1")
        rows.messages += MessageEntity(id = "m1", sessionId = "s1", rootId = "s1", authorKind = "user", body = "hello", createdAt = "2026-11-17T10:00:00.000Z")
        rows.messages += MessageEntity(id = "m2", sessionId = "s1", rootId = "s1", authorKind = "agent", body = "exp-sealed:v1:1:x", createdAt = "2026-11-17T10:00:01.000Z")
        rows.messages += MessageEntity(id = "dm", sessionId = null, body = "not a chat's", createdAt = "2026-11-17T10:00:02.000Z")
        lockedIds += "m2"
        val stores = manager()
        stores.start()
        waitUntil("the messages stored") { stores.openStoreForTests?.lockedMessageIds() == listOf("m2") }
        val found = Json.parseToJsonElement(stores.openStoreForTests!!.searchChat("s1", "hello", 10u)).jsonObject
        assertEquals(1, found.getValue("hits").jsonArray.size)
        // The key arrives: the sealed one opens and replaces its body-less row.
        lockedIds.clear()
        keys.value += 1
        waitUntil("the sealed message opened") { stores.openStoreForTests?.lockedMessageIds()?.isEmpty() == true }
    }

    private fun hits(stores: ChatStoreManager, root: String, query: String): Int =
        Json.parseToJsonElement(stores.openStoreForTests!!.searchChat(root, query, 10u)).jsonObject.getValue("hits").jsonArray.size

    @Test
    fun messagesAreTakenByArrivalNeverByCreatedAt() = runBlocking {
        // #EXP-2032 / #EXP-2018 (chat-store.json MESSAGES BY ARRIVAL).
        session("s1")
        rows.messages += MessageEntity(id = "m1", sessionId = "s1", rootId = "s1", authorKind = "user", body = "newest first", createdAt = "2026-11-17T10:00:00.000Z")
        val stores = manager()
        stores.start()
        waitUntil("the first message stored") { stores.openStoreForTests?.let { hits(stores, "s1", "newest") } == 1 }
        waitUntil("the mark at its position") { stores.openStoreForTests!!.meta(ChatStoreManager.MESSAGES_MARK) == "1" }
        // #EXP-2021: the open chat hears the store take its rows in.
        val tick = async { withTimeoutOrNull(10_000) { stores.taken("s1").first() } }
        delay(50)
        // A row that arrives LATER with an OLDER createdAt (a chat shared
        // later, an older shape page, a backfill) is taken like any new row.
        rows.messages += MessageEntity(id = "m0", sessionId = "s1", rootId = "s1", authorKind = "agent", body = "backfilled wombat", createdAt = "2026-11-14T08:00:00.000Z")
        rows.messagesStampFlow.value = "1"
        waitUntil("the late, older message stored") { hits(stores, "s1", "wombat") == 1 }
        assertNotNull("the store's take ticks the chat", tick.await())
        waitUntil("the mark moved to its position") { stores.openStoreForTests!!.meta(ChatStoreManager.MESSAGES_MARK) == "2" }
        // A replace moves a row to the end: its new body is taken again.
        rows.messages += MessageEntity(id = "m1", sessionId = "s1", rootId = "s1", authorKind = "user", body = "edited quokka", createdAt = "2026-11-17T10:00:00.000Z")
        rows.messagesStampFlow.value = "2"
        waitUntil("the replaced row taken") { hits(stores, "s1", "quokka") == 1 }
        // A refetch empties and refills the table: positions start at 1
        // again, under the stored mark, so the store starts over.
        rows.messages.clear()
        rows.messages += MessageEntity(id = "r1", sessionId = "s1", rootId = "s1", authorKind = "agent", body = "refilled platypus", createdAt = "2026-11-10T08:00:00.000Z")
        rows.messagesStampFlow.value = "3"
        waitUntil("the refilled row taken") { hits(stores, "s1", "platypus") == 1 }
        waitUntil("the mark started over") { stores.openStoreForTests!!.meta(ChatStoreManager.MESSAGES_MARK) == "1" }
    }

    @Test
    fun aLongBacklogPagesByPositionWithoutOffsets() = runBlocking {
        session("s1")
        // 1,200 rows, newest createdAt FIRST (arrival order != time order).
        repeat(1_200) { i ->
            rows.messages += MessageEntity(
                id = "m$i", sessionId = "s1", rootId = "s1", authorKind = "agent",
                body = if (i == 0 || i == 1_199) "wombatword $i" else "filler $i",
                createdAt = "2026-11-17T10:%02d:%02d.000Z".format(59 - (i / 60) % 60, 59 - i % 60),
            )
        }
        val stores = manager()
        stores.start()
        waitUntil("every row stored") { stores.openStoreForTests?.meta(ChatStoreManager.MESSAGES_MARK) == "1200" }
        assertEquals(2, hits(stores, "s1", "wombatword"))
        // Pages follow the positions (each after the last one taken).
        assertEquals(listOf(0L, 501L, 1001L), synchronized(rows.pages) { rows.pages.take(3).map { it.first } })
    }

    @Test
    fun theHolderAnswersUnderTheJoinsAccessRule() = runBlocking {
        session("s1", userId = "u1")
        net.registries["t1"] = listOf(
            TeamInstall(installId = "i1", userId = "u1", kind = "phone", endpointId = "asker", x25519Public = "x"),
            TeamInstall(installId = "i2", userId = "u2", kind = "phone", endpointId = "teammate", x25519Public = "x"),
        )
        val stores = manager()
        stores.start()
        waitUntil("the store open") { stores.openAccountId == "acct1" }
        stores.feedStore("s1").record((1..5).map { TailEvent(narration(it), it.toLong()) })
        stores.flushFeeds()
        val node = FakeNode()
        val page = buildJsonObject {
            put("t", "history_page")
            put("requestId", "cu-1")
            put("deviceId", "d1")
            put("limit", 200)
            put("have", kotlinx.serialization.json.JsonArray(emptyList()))
        }

        stores.answerPage(node, "asker", "s1", page)
        val chunk = answerFrom(node, "asker")
        assertEquals("history_chunk", chunk.tag())
        assertEquals("cu-1", (chunk["requestId"] as JsonPrimitive).content)
        assertEquals(5, chunk.getValue("events").jsonArray.size)
        assertNull(chunk["error"])

        // A teammate's install on a session that is not shared: refused.
        stores.answerPage(node, "teammate", "s1", page)
        assertEquals("unauthorized", (answerFrom(node, "teammate")["error"] as JsonPrimitive).content)
        // Shared: allowed.
        session("s1", userId = "u1", shared = true)
        stores.answerPage(node, "teammate", "s1", page)
        assertNull(answerFrom(node, "teammate")["error"])
        // No synced row here.
        stores.answerPage(node, "asker", "nope", page)
        assertEquals("no_such_session", (answerFrom(node, "asker")["error"] as JsonPrimitive).content)
        // A viewer's page (no deviceId) is not the holder's.
        val sentBefore = node.sent.size
        stores.answerPage(node, "asker", "s1", JsonObject(page - "deviceId"))
        assertEquals(sentBefore, node.sent.size)
        assertEquals(PageAccess.UNAUTHORIZED, pageAccess(true, "u1", false, null))
    }

    @Test
    fun aPendingChunkGoesToTheWalkNeverToAViewer() = runBlocking {
        val node = FakeNode()
        val socket = EnetSteerSocket(node, "s1", "device", SteerEnetTimings(), { 0L }, Dispatchers.Unconfined)
        val answer = CatchUpChunks.expect("cu-7")
        val chunk = buildJsonObject {
            put("t", "history_chunk")
            put("requestId", "cu-7")
            put("deviceId", "d1")
            put("events", kotlinx.serialization.json.JsonArray(emptyList()))
        }
        val asked = buildJsonObject {
            put("t", "history_page")
            put("requestId", "cu-8")
            put("deviceId", "d1")
        }
        val viewerChunk = buildJsonObject {
            put("t", "history_chunk")
            put("requestId", "p1")
        }
        val synced = buildJsonObject { put("t", "activity_synced") }
        val probe = scope.async { socket.probe() }
        waitUntil("the join") { node.sent.isNotEmpty() }
        node.incoming.emit(PeerChat("device", encodeSteerEnvelope("s1", listOf(chunk, asked, viewerChunk, synced))))
        assertEquals("cu-7", (Json.parseToJsonElement(answer.await()).jsonObject["requestId"] as JsonPrimitive).content)
        assertFalse(CatchUpChunks.isPending("cu-7"))
        // The viewer's own page answer reaches it; the catch-up frames never do.
        assertEquals(EnetJoin.Joined, probe.await())
        val seen = listOf(socket.incoming.receive(), socket.incoming.receive())
            .map { (Json.parseToJsonElement(it).jsonObject["t"] as JsonPrimitive).content to Json.parseToJsonElement(it).jsonObject["requestId"]?.let { id -> (id as JsonPrimitive).content } }
        assertEquals(listOf("history_chunk" to "p1", "activity_synced" to null), seen)
        socket.cancel()
        assertTrue(ChatStoreManager.mentionsHistory(encodeSteerEnvelope("s1", listOf(chunk))))
        assertFalse(ChatStoreManager.mentionsHistory("""{"t":"exp-steer","frames":[{"t":"activity"}]}""".encodeToByteArray()))
    }

    @Test
    fun aPassWalksTheRunnerAndStoresWhatItLacks() = runBlocking {
        // The runner's store holds 1..450 of s1 on d1; the phone holds none.
        val runner = ChatStore.inMemory()
        runner.putFeed("s1", "s1", "d1", ChatStoreCodec.feedJson((1..450).map { TailEvent(narration(it), it.toLong()) }), now)
        val phone = ChatStore.inMemory()
        val walk = CatchUp(phone, "", "u1", 30u)
        walk.setInstalls("me", listOf(CatchUpInstall("runner", "u1", "d1", "2026-11-17T10:00:00Z")))
        val asked = mutableListOf<String>()
        val driver = CatchUpDriver(
            catchUp = walk,
            carrier = { peer, sessionId, frame ->
                asked += peer
                // The runner answers off its store (fullTo = its completeTo: a reader's store).
                val answer = runner.answerPage(sessionId, frame, PageAccess.ALLOWED)!!
                val envelope = parseSteerEnvelope(encodeSteerEnvelope(sessionId, listOf(Json.parseToJsonElement(answer).jsonObject)).decodeToString())!!
                envelope.frames.forEach { CatchUpChunks.take(it) }
                true
            },
            nowMs = { now },
        )
        val due = driver.pass(listOf(CatchUpSession("s1", "s1", "d1", "2026-11-17T10:00:00Z", false)))
        assertEquals(1, due)
        assertEquals(450uL, phone.count("s1", "d1"))
        assertTrue(asked.all { it == "runner" })
        assertTrue(asked.size >= 3)
        walk.close()
        runner.close()
        phone.close()
    }

    private fun JsonObject.tag() = (this["t"] as? JsonPrimitive)?.contentOrNull

    /** The last envelope sent to [peer], its one frame. */
    private fun answerFrom(node: FakeNode, peer: String): JsonObject {
        val body = synchronized(node.sent) { node.sent.last { it.first == peer }.second }
        return parseSteerEnvelope(body)!!.frames.single()
    }
}
