package com.exponential.app.ui.apps

import at.exponential.engine.ffi.Submitted
import at.exponential.engine.ffi.SyncTransport
import at.exponential.engine.ffi.SyncedRecordStore
import com.exponential.app.data.db.AppRecordOutboxDao
import com.exponential.app.data.db.AppRecordOutboxEntity
import com.exponential.app.domain.AppRecords
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** VAPP-272: the Room log under the real Engine (its host library, loaded
 *  through JNA): what an installed app already queued reads back, and what the
 *  Engine writes reads as the host's own rows. */
class AppRecordOutboxLogTest {
    private class Rows(val rows: MutableList<AppRecordOutboxEntity> = mutableListOf()) : AppRecordOutboxDao {
        override fun observeAll(): Flow<List<AppRecordOutboxEntity>> = flowOf(rows)
        override fun observeByApp(appId: String): Flow<List<AppRecordOutboxEntity>> = flowOf(rows.filter { it.appId == appId })
        override suspend fun all() = rows.sortedBy { it.seq }
        override suspend fun maxSeq() = rows.maxOfOrNull { it.seq } ?: 0L
        override suspend fun insert(item: AppRecordOutboxEntity) { rows.add(item) }
        override suspend fun ack(writeId: String, ackedAt: String) {
            rows.replaceAll { if (it.writeId == writeId) it.copy(state = "acked", ackedAt = ackedAt) else it }
        }
        override suspend fun delete(writeIds: List<String>) { rows.removeAll { it.writeId in writeIds } }
    }

    private class Replies(vararg replies: String) : SyncTransport {
        val replies = ArrayDeque(replies.toList())
        val sent = mutableListOf<String>()
        override fun push(app: String, call: String, inputJson: String): String {
            sent.add(call)
            return replies.removeFirstOrNull() ?: """{"transport":true}"""
        }
    }

    private fun engine() {
        val dir = System.getProperty("jna.library.path").orEmpty()
        assumeTrue("the Engine's host library", listOf("dylib", "so").any { File(dir, "libeengine_ffi.$it").exists() })
    }

    /** A row as a shipped app wrote it. */
    private fun stored(writeId: String, seq: Long, writes: List<AppRecords.OutboxWrite>, call: String = "batch") = AppRecordOutboxEntity(
        writeId = writeId,
        seq = seq,
        appId = "a",
        call = call,
        writes = AppRecords.outboxWritesJson(writes),
        at = "2026-11-07T09:00:00.000Z",
        state = "pending",
    )

    @Test
    fun `a queue an installed app left replays in order and acks its rows`() {
        engine()
        val rows = Rows()
        rows.rows.add(stored("w1", 1, listOf(AppRecords.OutboxWrite("create", "r1", "task", emptyMap()))))
        rows.rows.add(stored("w2", 2, listOf(AppRecords.OutboxWrite("update", "r1", "", emptyMap()), AppRecords.OutboxWrite("delete", "r2"))))
        val transport = Replies("""{"ok":true}""", """{"ok":true}""")
        val store = SyncedRecordStore(RoomLog(rows), transport, null, null)
        assertEquals("2 changes waiting to sync", store.pendingLabel("a"))
        store.flush()
        assertEquals(listOf("batch", "batch"), transport.sent)
        assertEquals(listOf("acked", "acked"), rows.rows.map { it.state })
    }

    @Test
    fun `what the Engine queues reads as the host's own row`() {
        engine()
        val rows = Rows()
        val store = SyncedRecordStore(RoomLog(rows), Replies(), null, null)
        val answer = store.submitCall("a", "records_upsert", """{"kind":"task","data":{"title":"t"}}""", false)
        val row = rows.rows.single()
        val entry = AppRecordOutbox.entryOf(row)
        assertEquals(Submitted.Queued("""{"id":"${entry.writes.single().id}"}"""), answer)
        assertEquals(listOf("upsert", "create", "task", 1L), listOf(row.call, entry.writes.single().op, entry.writes.single().kind, row.seq))
        assertEquals("pending", row.state)
    }

    @Test
    fun `an unreadable row leaves and the rest still run`() {
        engine()
        val rows = Rows()
        rows.rows.add(stored("bad", 1, emptyList()).copy(writes = "{not json"))
        rows.rows.add(stored("w1", 2, listOf(AppRecords.OutboxWrite("delete", "r1")), call = "delete"))
        val store = SyncedRecordStore(RoomLog(rows), Replies(), null, null)
        assertEquals("1 change waiting to sync", store.pendingLabel("a"))
        assertEquals(listOf("w1"), rows.rows.map { it.writeId })
    }

    /** Valid JSON the Engine still can't read (an unknown call, a create
     *  without a kind) used to stay in Room: skipped by the Engine, pending and
     *  overlaying its rows (and counted by the header glyph) forever. */
    @Test
    fun `a row the Engine can't read leaves too and the good one replays`() {
        engine()
        val rows = Rows()
        val create = listOf(AppRecords.OutboxWrite("create", "r1", "entry", emptyMap()))
        rows.rows.add(stored("bad-call", 1, create, call = "frobnicate"))
        rows.rows.add(stored("bad-kind", 2, emptyList()).copy(writes = """[{"op":"create","id":"r2","data":{}}]"""))
        rows.rows.add(stored("bad-state", 3, create).copy(state = "sent"))
        rows.rows.add(stored("no-write", 4, emptyList(), call = "upsert"))
        rows.rows.add(stored("w1", 5, create))
        val transport = Replies("""{"ok":true}""")
        val store = SyncedRecordStore(RoomLog(rows), transport, null, null)
        assertEquals("1 change waiting to sync", store.pendingLabel("a"))
        assertEquals(listOf("w1"), rows.rows.map { it.writeId })
        store.flush()
        assertEquals(listOf("batch"), transport.sent)
        assertEquals(listOf("acked"), rows.rows.map { it.state })
        assertEquals(null, AppRecords.outboxPendingLabel(rows.rows.map(AppRecordOutbox::entryOf), "a"))
    }
}
