package com.exponential.app.data.db

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
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
 * SLOP-4: [MIGRATION_79_80] on a real version-79 database, built like
 * [Migration78To79Test]: today's schema from Room, with `comments` and `teams`
 * swapped back to their v79 shape. Room opens it with the migration and NO
 * destructive fallback, so the hand-copied v80 DDL is validated against the
 * entities. Rows survive; the comments offset is marked for the refetch that
 * restores reporter rows an older build dropped, every other offset stays.
 */
@RunWith(AndroidJUnit4::class)
class Migration79To80Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun keepsRowsAndRefetchesOnlyTheCommentsShape() {
        createVersion79()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_79_80)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(80, sql.version)

            sql.query("SELECT id, author_id, source, audience, body FROM comments").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("c1", it.getString(0))
                assertEquals("u1", it.getString(1))
                assertEquals("mcp", it.getString(2))
                assertEquals("team", it.getString(3))
                assertEquals("hi", it.getString(4))
            }
            sql.query("SELECT id, name, yolo_mode FROM teams").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("t1", it.getString(0))
                assertEquals("Acme", it.getString(1))
                assertEquals(1, it.getInt(2))
            }
            sql.query("PRAGMA table_info(`teams`)").use {
                val columns = ArrayList<String>()
                while (it.moveToNext()) columns.add(it.getString(it.getColumnIndexOrThrow("name")))
                assertFalse("helpdesk_enabled" in columns)
            }
            // A reporter comment (no users row) now fits the table.
            sql.execSQL(
                "INSERT INTO comments (id, issue_id, team_id, author_id, source, audience, kind, " +
                    "created_at, updated_at) VALUES ('c2', 'i1', 't1', NULL, 'reporter', 'team', " +
                    "'regular', 'c', 'u')",
            )

            val offsets = db.electricOffsetDao()
            val comments = runBlocking { offsets.get("comments") }!!
            assertEquals("", comments.handle)
            assertEquals("-1", comments.offset)
            assertFalse(comments.isLive)
            assertTrue(comments.needsRefetch)
            for (shape in listOf("teams", "issues")) {
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

    /** A v79 file: today's schema (from Room) with v79's comments + teams. */
    private fun createVersion79() {
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
            db.execSQL("DROP TABLE comments")
            db.execSQL(
                "CREATE TABLE `comments` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, " +
                    "`team_id` TEXT NOT NULL, `board_id` TEXT, `author_id` TEXT NOT NULL, " +
                    "`parent_id` TEXT, `source` TEXT, `body` TEXT, `kind` TEXT NOT NULL, " +
                    "`edited_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, " +
                    "PRIMARY KEY(`id`))",
            )
            db.execSQL("DROP TABLE teams")
            db.execSQL(
                "CREATE TABLE `teams` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, " +
                    "`slug` TEXT NOT NULL, `icon_url` TEXT, `helpdesk_enabled` INTEGER NOT NULL, " +
                    "`yolo_mode` INTEGER NOT NULL, `estimation_type` TEXT, " +
                    "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO comments (id, issue_id, team_id, author_id, source, body, kind, " +
                    "created_at, updated_at) VALUES ('c1', 'i1', 't1', 'u1', 'mcp', 'hi', " +
                    "'regular', 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO teams (id, name, slug, helpdesk_enabled, yolo_mode, created_at, " +
                    "updated_at) VALUES ('t1', 'Acme', 'acme', 1, 1, 'c', 'u')",
            )
            for (shape in listOf("comments", "teams", "issues")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 79
        }
    }

    private companion object {
        const val SCHEMA_DB = "slop4-schema-v80.db"
        const val TEST_DB = "slop4-migration-v79.db"
    }
}
