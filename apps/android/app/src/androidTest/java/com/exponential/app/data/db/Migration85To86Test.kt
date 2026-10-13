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
 * EXP-1284: [MIGRATION_85_86] on a real version-85 database, built like
 * [Migration79To80Test]: today's schema from Room with `issue_drafts` swapped
 * back to its v85 shape (no `thread_id`). Room opens it with the migration and
 * NO destructive fallback, so the added column is validated against the
 * entity. Drafts survive with a null thread; the issue-drafts offset is marked
 * for the refetch that re-delivers them carrying it, every other offset stays.
 */
@RunWith(AndroidJUnit4::class)
class Migration85To86Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun keepsDraftsAndRefetchesOnlyTheIssueDraftsShape() {
        createVersion85()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_85_86)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(86, sql.version)

            val draft = runBlocking { db.issueDraftDao().observeById("d1").first() }!!
            assertEquals("Draft", draft.title)
            assertEquals("b1", draft.boardId)
            assertEquals(listOf("l1"), draft.labelIds)
            assertNull(draft.threadId)
            // A thread draft now fits the table.
            runBlocking { db.issueDraftDao().upsert(draft.copy(id = "d2", threadId = "th1")) }
            assertEquals("th1", runBlocking { db.issueDraftDao().observeById("d2").first() }?.threadId)

            // The thread rows synced at v85 stay.
            sql.query("SELECT id FROM threads").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("th1", it.getString(0))
            }

            val offsets = db.electricOffsetDao()
            val drafts = runBlocking { offsets.get("issue_drafts") }!!
            assertEquals("", drafts.handle)
            assertEquals("-1", drafts.offset)
            assertFalse(drafts.isLive)
            assertTrue(drafts.needsRefetch)
            for (shape in listOf("threads", "thread_items", "issues")) {
                val kept = runBlocking { offsets.get(shape) }!!
                assertEquals("h", kept.handle)
                assertEquals("0_0", kept.offset)
                assertTrue(kept.isLive)
                assertFalse(kept.needsRefetch)
            }
            assertNull(runBlocking { offsets.get("labels") })
        } finally {
            db.close()
        }
    }

    /** A v85 file: today's schema (from Room) with v85's `issue_drafts`. */
    private fun createVersion85() {
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
            val drafts = schema.first { it.startsWith("CREATE TABLE") && "`issue_drafts`" in it.substringBefore("(") }
            val draftsV85 = drafts.replace("`thread_id` TEXT, ", "")
            assertFalse("thread_id" in draftsV85)
            db.execSQL("DROP TABLE issue_drafts")
            db.execSQL(draftsV85)
            for (index in schema.filter { it.startsWith("CREATE INDEX") && "`issue_drafts`" in it }) {
                db.execSQL(index)
            }
            db.execSQL(
                "INSERT INTO issue_drafts (id, user_id, team_id, board_id, title, description, " +
                    "priority, label_ids, created_at, updated_at) VALUES ('d1', 'u1', 't1', 'b1', " +
                    "'Draft', '', 'none', '{l1}', 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO threads (id, team_id, created_at, updated_at) VALUES ('th1', 't1', 'c', 'u')",
            )
            for (shape in listOf("issue_drafts", "threads", "thread_items", "issues")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 85
        }
    }

    private companion object {
        const val SCHEMA_DB = "exp1284-schema-v86b.db"
        const val TEST_DB = "exp1284-migration-v85.db"
    }
}
