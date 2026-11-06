package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1715: v120 adds `sessions.scope_team_ids` (nullable) and refetches
 * ONLY the sessions shape; the row decodes it tolerant of absence.
 */
class ScopeTeamIdsMigrationTest {
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
    fun `v120 adds scope_team_ids and refetches only sessions`() {
        assertEquals(119, MIGRATION_119_120.startVersion)
        assertEquals(120, MIGRATION_119_120.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_119_120.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `sessions` ADD COLUMN `scope_team_ids` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
            ),
            statements,
        )
    }

    @Test
    fun `the row decodes scope_team_ids, absent or null = every team`() {
        val json = Json { ignoreUnknownKeys = true }
        fun decode(raw: String) = json.decodeFromString(SessionEntity.serializer(), raw)
        assertEquals(listOf("t1", "t2"), decode("""{"id":"s1","scope_team_ids":"{t1,t2}"}""").scopeTeamIdList)
        assertEquals(listOf("t1"), decode("""{"id":"s1","scopeTeamIds":["t1"]}""").scopeTeamIdList)
        assertNull(decode("""{"id":"s1","scope_team_ids":null}""").scopeTeamIdList)
        assertNull(decode("""{"id":"s1"}""").scopeTeamIdList)
    }
}
