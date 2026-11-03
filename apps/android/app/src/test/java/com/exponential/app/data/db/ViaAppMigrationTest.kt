package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #VAPP-229: v114 adds `via_app_id` (nullable) to issues, sessions and
 * app_records and refetches ONLY those three shapes.
 */
class ViaAppMigrationTest {
    /** A database that records every `execSQL` (nothing else is called). */
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
    fun `v114 adds via_app_id and refetches only issues, sessions and app_records`() {
        assertEquals(113, MIGRATION_113_114.startVersion)
        assertEquals(114, MIGRATION_113_114.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_113_114.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `issues` ADD COLUMN `via_app_id` TEXT",
                "ALTER TABLE `sessions` ADD COLUMN `via_app_id` TEXT",
                "ALTER TABLE `app_records` ADD COLUMN `via_app_id` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` IN ('issues', 'sessions', 'app_records')",
            ),
            statements,
        )
    }

    @Test
    fun `the rows decode via_app_id`() {
        val json = Json { ignoreUnknownKeys = true }
        val session = json.decodeFromString(SessionEntity.serializer(), """{"id":"s1","via_app_id":"a1"}""")
        assertEquals("a1", session.viaAppId)
        assertNull(json.decodeFromString(SessionEntity.serializer(), """{"id":"s1"}""").viaAppId)
        val record = json.decodeFromString(AppRecordEntity.serializer(), """{"id":"r1","viaAppId":"a2"}""")
        assertEquals("a2", record.viaAppId)
    }
}
