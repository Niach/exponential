package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * VAPP-201: v112 adds `teams.time_zone` (nullable, matching [TeamEntity]'s
 * Room schema) and refetches ONLY the teams shape, so synced rows re-arrive
 * carrying it.
 */
class TeamTimeZoneMigrationTest {
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
    fun `v112 adds teams time_zone and refetches only the teams shape`() {
        assertEquals(111, MIGRATION_111_112.startVersion)
        assertEquals(112, MIGRATION_111_112.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_111_112.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `teams` ADD COLUMN `time_zone` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'teams'",
            ),
            statements,
        )
    }

    @Test
    fun `the team row decodes time_zone`() {
        val json = Json { ignoreUnknownKeys = true }
        val row = json.decodeFromString(
            TeamEntity.serializer(),
            """{"id":"t1","name":"Acme","slug":"acme","time_zone":"Europe/Vienna","created_at":"x","updated_at":"x"}""",
        )
        assertEquals("Europe/Vienna", row.timeZone)
        val without = json.decodeFromString(
            TeamEntity.serializer(),
            """{"id":"t1","name":"Acme","slug":"acme","created_at":"x","updated_at":"x"}""",
        )
        assertNull(without.timeZone)
    }
}
