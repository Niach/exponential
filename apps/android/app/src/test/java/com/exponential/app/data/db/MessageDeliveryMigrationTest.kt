package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

/** #FEED-237: v126 adds `messages.delivered_at` + the LOCAL `outbox` table (only messages refetch); v127 its `pending_files`. */
class MessageDeliveryMigrationTest {
    private fun recording(statements: MutableList<String>): SupportSQLiteDatabase =
        Proxy.newProxyInstance(
            SupportSQLiteDatabase::class.java.classLoader,
            arrayOf(SupportSQLiteDatabase::class.java),
        ) { _, method, args ->
            if (method.name == "execSQL") {
                statements += args!![0] as String
                null
            } else {
                error("unexpected ${method.name}")
            }
        } as SupportSQLiteDatabase

    @Test
    fun `v126 adds delivered_at and the outbox table and refetches only messages`() {
        assertEquals(125, MIGRATION_125_126.startVersion)
        assertEquals(126, MIGRATION_125_126.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_125_126.migrate(recording(statements))
        assertEquals(4, statements.size)
        assertEquals("ALTER TABLE `messages` ADD COLUMN `delivered_at` TEXT", statements[0])
        assert(statements[1].startsWith("CREATE TABLE IF NOT EXISTS `outbox`"))
        assert(statements[2].contains("index_outbox_session_id"))
        assert(statements[3].endsWith("WHERE `shape` = 'messages'"))
    }

    @Test
    fun `v127 adds the outbox's pending files and refetches nothing`() {
        assertEquals(126, MIGRATION_126_127.startVersion)
        assertEquals(127, MIGRATION_126_127.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_126_127.migrate(recording(statements))
        assertEquals(listOf("ALTER TABLE `outbox` ADD COLUMN `pending_files` TEXT"), statements)
    }

    @Test
    fun `delivered_at syncs off the shape`() {
        val json = Json { ignoreUnknownKeys = true }
        val row = json.decodeFromString(
            MessageEntity.serializer(),
            """{"id":"m1","body":"hi","delivered_at":"2026-11-13T10:00:01.000Z"}""",
        )
        assertEquals("2026-11-13T10:00:01.000Z", row.deliveredAt)
    }
}
