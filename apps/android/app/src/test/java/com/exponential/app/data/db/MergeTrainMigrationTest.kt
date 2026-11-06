package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1710: v119 adds the merge train's synced columns (pull_requests.merge_*,
 *  teams.merge_mode) and refetches ONLY those two shapes; the rows decode them
 *  from Electric's text forms, tRPC's camelCase and their absence. */
class MergeTrainMigrationTest {
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
    fun `v119 adds the merge columns and refetches only pull_requests and teams`() {
        assertEquals(MIGRATION_118_119.startVersion + 1, MIGRATION_118_119.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_118_119.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_state` TEXT NOT NULL DEFAULT 'checks'",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_wait` TEXT",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_attempts` INTEGER",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_hold` TEXT",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_queued_since` TEXT",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_now` INTEGER NOT NULL DEFAULT 0",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_position` INTEGER",
                "ALTER TABLE `pull_requests` ADD COLUMN `merge_train_id` TEXT",
                "ALTER TABLE `teams` ADD COLUMN `merge_mode` TEXT NOT NULL DEFAULT 'auto'",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` IN ('pull_requests', 'teams')",
            ),
            statements,
        )
    }

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `a pull request row decodes the merge columns`() {
        val row = json.decodeFromString(
            PullRequestEntity.serializer(),
            """{"id":"pr1","merge_state":"queued","merge_wait":null,"merge_attempts":"2","merge_hold":"bounces",
               "merge_queued_since":"2027-01-15 08:00:00+00","merge_now":"t","merge_position":"2","merge_train_id":"tr1"}""",
        )
        assertEquals("queued", row.mergeState)
        assertNull(row.mergeWait)
        assertEquals(2, row.mergeAttempts)
        assertEquals("bounces", row.mergeHold)
        assertEquals("2027-01-15 08:00:00+00", row.mergeQueuedSince)
        assertTrue(row.mergeNow)
        assertEquals(2, row.mergePosition)
        assertEquals("tr1", row.mergeTrainId)
        val camel = json.decodeFromString(
            PullRequestEntity.serializer(),
            """{"id":"pr1","mergeState":"testing","mergeWait":"approval","mergeNow":false,"mergePosition":3,"mergeTrainId":"tr2"}""",
        )
        assertEquals("testing", camel.mergeState)
        assertEquals("approval", camel.mergeWait)
        assertFalse(camel.mergeNow)
        assertEquals(3, camel.mergePosition)
        val bare = json.decodeFromString(PullRequestEntity.serializer(), """{"id":"pr1"}""")
        assertEquals("checks", bare.mergeState)
        assertFalse(bare.mergeNow)
        assertNull(bare.mergePosition)
        assertNull(bare.mergeTrainId)
    }
}
