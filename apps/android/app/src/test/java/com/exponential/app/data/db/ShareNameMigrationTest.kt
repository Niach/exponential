package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** VAPP-241: v118 adds `app_records.share_name` (nullable) and refetches ONLY that shape. */
class ShareNameMigrationTest {
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
    fun `v118 adds share_name and refetches only app_records`() {
        assertEquals(117, MIGRATION_117_118.startVersion)
        assertEquals(118, MIGRATION_117_118.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_117_118.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `app_records` ADD COLUMN `share_name` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'app_records'",
            ),
            statements,
        )
    }

    @Test
    fun `the rows decode share_name`() {
        val json = Json { ignoreUnknownKeys = true }
        assertEquals("signup", json.decodeFromString(AppRecordEntity.serializer(), """{"id":"r1","share_name":"signup"}""").shareName)
        assertEquals("signup", json.decodeFromString(AppRecordEntity.serializer(), """{"id":"r1","shareName":"signup"}""").shareName)
        assertNull(json.decodeFromString(AppRecordEntity.serializer(), """{"id":"r1"}""").shareName)
    }
}
