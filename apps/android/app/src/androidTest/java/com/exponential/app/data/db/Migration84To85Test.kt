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
 * EXP-1284: [MIGRATION_84_85] on a real version-84 database, built like
 * [Migration79To80Test]: today's schema from Room with `threads` and
 * `thread_items` dropped (v84 had neither) and `issue_drafts` swapped back to
 * its pre-`thread_id` shape. Room opens it the way an upgrading install does
 * (84 → 85 → 86) with NO destructive fallback, so the hand-copied v85 DDL is
 * validated against the entities. Both new tables land empty and usable;
 * every existing row and every shape's offset survives 84 → 85 (the
 * `issue_drafts` refetch is [MIGRATION_85_86]'s, see [Migration85To86Test]),
 * and the two new shapes have no offset row, so they snapshot from scratch.
 */
@RunWith(AndroidJUnit4::class)
class Migration84To85Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun createsEmptyThreadTablesAndKeepsEveryRowAndOffset() {
        createVersion84()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_84_85, MIGRATION_85_86)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(86, sql.version)

            // The new tables exist, empty, with their indices.
            assertTrue(runBlocking { db.threadDao().observeAll().first() }.isEmpty())
            assertTrue(runBlocking { db.threadItemDao().observeAll().first() }.isEmpty())
            sql.query(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name IN ('threads', 'thread_items')",
            ).use {
                val indices = HashSet<String>()
                while (it.moveToNext()) indices.add(it.getString(0))
                assertTrue("index_threads_team_id" in indices)
                assertTrue("index_thread_items_thread_id" in indices)
                assertTrue("index_thread_items_issue_id" in indices)
                assertTrue("index_thread_items_session_id" in indices)
                assertTrue("index_thread_items_team_id" in indices)
            }

            // ...and take the rows the two new shapes deliver.
            runBlocking {
                db.threadDao().upsert(
                    ThreadEntity(id = "th1", teamId = "t1", title = "Fix login", createdAt = "c", updatedAt = "u"),
                )
                db.threadItemDao().upsert(
                    ThreadItemEntity(
                        id = "ti1", threadId = "th1", kind = "session", sessionId = "s1", teamId = "t1",
                        sortOrder = 1.0, createdAt = "c", updatedAt = "u",
                    ),
                )
            }
            val thread = runBlocking { db.threadDao().observeOfSession("s1").first() }
            assertEquals("th1", thread?.id)
            assertEquals("Fix login", thread?.title)

            // The existing draft row survives (85 → 86 adds its null thread).
            sql.query("SELECT id, title, label_ids, thread_id FROM issue_drafts").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("d1", it.getString(0))
                assertEquals("Draft", it.getString(1))
                assertEquals("{l1}", it.getString(2))
                assertTrue(it.isNull(3))
            }

            val offsets = db.electricOffsetDao()
            for (shape in listOf("teams", "issues", "coding_sessions")) {
                val kept = runBlocking { offsets.get(shape) }!!
                assertEquals("h", kept.handle)
                assertEquals("0_0", kept.offset)
                assertTrue(kept.isLive)
                assertFalse(kept.needsRefetch)
            }
            assertNull(runBlocking { offsets.get("threads") })
            assertNull(runBlocking { offsets.get("thread_items") })
        } finally {
            db.close()
        }
    }

    /** A v84 file: today's schema (from Room) minus the thread tables, with
     *  v84's `issue_drafts` (no `thread_id`). */
    private fun createVersion84() {
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
            for (statement in schema) db.execSQL(statement)
            // DROP TABLE takes the tables' indices with it.
            db.execSQL("DROP TABLE threads")
            db.execSQL("DROP TABLE thread_items")
            val drafts = schema.first { it.startsWith("CREATE TABLE") && "`issue_drafts`" in it.substringBefore("(") }
            val draftsV84 = drafts.replace("`thread_id` TEXT, ", "")
            assertFalse("thread_id" in draftsV84)
            db.execSQL("DROP TABLE issue_drafts")
            db.execSQL(draftsV84)
            for (index in schema.filter { it.startsWith("CREATE INDEX") && "`issue_drafts`" in it }) {
                db.execSQL(index)
            }
            db.execSQL(
                "INSERT INTO issue_drafts (id, user_id, team_id, board_id, title, description, " +
                    "priority, label_ids, created_at, updated_at) VALUES ('d1', 'u1', 't1', 'b1', " +
                    "'Draft', '', 'none', '{l1}', 'c', 'u')",
            )
            for (shape in listOf("teams", "issues", "coding_sessions")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 84
        }
    }

    private companion object {
        const val SCHEMA_DB = "exp1284-schema-v86.db"
        const val TEST_DB = "exp1284-migration-v84.db"
    }
}
