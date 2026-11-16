package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import com.exponential.app.domain.SessionTree
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #NEW-49: v130 adds `sessions.watching` (nullable jsonb TEXT) and refetches
 * ONLY the sessions shape; the row decodes it tolerant of absence.
 */
class WatchingMigrationTest {
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
    fun `v130 adds watching and refetches only sessions`() {
        assertEquals(129, MIGRATION_129_130.startVersion)
        assertEquals(130, MIGRATION_129_130.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_129_130.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `sessions` ADD COLUMN `watching` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
            ),
            statements,
        )
    }

    @Test
    fun `the row decodes watching, absent or null = nothing`() {
        val json = Json { ignoreUnknownKeys = true }
        fun decode(raw: String) = json.decodeFromString(SessionEntity.serializer(), raw)
        val row = decode("""{"id":"s1","watching":[{"id":"w1","kind":"pr_checks","subjectId":"p1","label":"CI #1130"}]}""")
        assertEquals(
            listOf(SessionTree.Watching("w1", "pr_checks", "p1", "CI #1130")),
            SessionTree.session(row).watching,
        )
        assertNull(decode("""{"id":"s1","watching":null}""").watching)
        assertNull(decode("""{"id":"s1"}""").watching)
        assertEquals(emptyList<SessionTree.Watching>(), SessionTree.session(decode("""{"id":"s1"}""")).watching)
    }
}
