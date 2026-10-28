package com.exponential.app.data.db

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EXP-1388: [MIGRATION_101_102] on a real version-101 database, built from
 * today's schema with `session_reads` minus `archived_at`. Room opens it with
 * the migration and NO destructive fallback, so the column is validated
 * against [SessionReadEntity]; the existing read is not archived, an archive
 * round-trips, and only the `session_reads` offset resets to refetch.
 */
@RunWith(AndroidJUnit4::class)
class Migration101To102Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun addsTheArchivedAtColumn() {
        createVersion101()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_101_102, MIGRATION_102_103, MIGRATION_103_104)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(104, sql.version)

            val reads = db.sessionReadDao()
            val read = runBlocking { reads.observeAll().first() }.single()
            assertEquals("s1", read.sessionId)
            assertNull(read.archivedAt)

            runBlocking { reads.upsert(read.copy(archivedAt = "2026-10-27T10:00:00.000Z")) }
            assertEquals(
                "2026-10-27T10:00:00.000Z",
                runBlocking { reads.observeAll().first() }.single().archivedAt,
            )

            // Only the session_reads shape refetches, so synced rows re-arrive archived.
            val refetched = runBlocking { db.electricOffsetDao().get("session_reads") }!!
            assertEquals("", refetched.handle)
            assertEquals("-1", refetched.offset)
            assertFalse(refetched.isLive)
            assertTrue(refetched.needsRefetch)
            val kept = runBlocking { db.electricOffsetDao().get("sessions") }!!
            assertEquals("h", kept.handle)
            assertTrue(kept.isLive)
        } finally {
            db.close()
        }
    }

    /** A v101 file: today's schema (from Room) with `session_reads` minus `archived_at`. */
    private fun createVersion101() {
        val schema = ArrayList<String>()
        val fresh = Room.databaseBuilder(context, ExponentialDatabase::class.java, SCHEMA_DB).build()
        try {
            fresh.openHelper.writableDatabase.query(
                "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL " +
                    "AND name NOT LIKE 'sqlite_%' AND name != 'android_metadata' " +
                    "ORDER BY CASE type WHEN 'table' THEN 0 ELSE 1 END",
            ).use { while (it.moveToNext()) schema.add(it.getString(0)) }
        } finally {
            fresh.close()
            context.deleteDatabase(SCHEMA_DB)
        }

        val file = context.getDatabasePath(TEST_DB)
        file.parentFile?.mkdirs()
        SQLiteDatabase.openOrCreateDatabase(file, null).use { db ->
            for (statement in schema) {
                val reads = statement.startsWith("CREATE TABLE `session_reads`")
                val v100 = if (reads) statement.replace(Regex(""",\s*`(archived_at|story_seen_at)` TEXT"""), "") else statement
                if (reads) assertTrue(!v100.contains("archived_at"))
                db.execSQL(v100)
            }
            db.execSQL(
                "INSERT INTO session_reads (id, user_id, session_id, read_at, created_at, updated_at) " +
                    "VALUES ('r1', 'u1', 's1', '2026-10-27T09:00:00.000Z', 'c', 'u')",
            )
            for (shape in listOf("session_reads", "sessions")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 101
        }
    }

    private companion object {
        const val SCHEMA_DB = "exp1388-schema-v102.db"
        const val TEST_DB = "exp1388-migration-v101.db"
    }
}
