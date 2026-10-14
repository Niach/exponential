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
 * EXP-1291: [MIGRATION_86_87] on a real version-86 database, built from
 * today's schema with v86's extras put back: the `coding_sessions`,
 * `threads` and `thread_items` tables, the issues pr_* / branch columns and
 * the actions `repository_id` / `triggers`. Room opens it with the migration
 * and NO destructive fallback, so every rebuilt / created table is validated
 * against its entity. Issues and actions survive; the dropped tables and
 * their offsets go; every other offset stays.
 */
@RunWith(AndroidJUnit4::class)
class Migration86To87Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun dropsTheOldTreeAndKeepsIssuesAndActions() {
        createVersion86()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_86_87)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(87, sql.version)

            val issue = runBlocking { db.issueDao().observeById("i1").first() }!!
            assertEquals("ACME-1", issue.identifier)
            val action = runBlocking { db.actionDao().observeById("a1").first() }!!
            assertEquals("Nightly", action.name)
            assertTrue(runBlocking { db.sessionDao().observeAll().first() }.isEmpty())
            assertTrue(runBlocking { db.messageDao().observeAll().first() }.isEmpty())

            sql.query("SELECT name FROM sqlite_master WHERE type = 'table'").use {
                val tables = ArrayList<String>()
                while (it.moveToNext()) tables.add(it.getString(0))
                for (dropped in listOf("coding_sessions", "threads", "thread_items")) assertFalse(dropped in tables)
                for (created in listOf("sessions", "session_issues", "messages", "pull_requests", "pull_request_issues")) {
                    assertTrue(created in tables)
                }
            }
            val offsets = db.electricOffsetDao()
            for (dropped in listOf("coding_sessions", "threads", "thread_items", "_repair:x:threads")) {
                assertNull(runBlocking { offsets.get(dropped) })
            }
            for (kept in listOf("issues", "actions")) {
                val row = runBlocking { offsets.get(kept) }!!
                assertEquals("h", row.handle)
                assertTrue(row.isLive)
            }
        } finally {
            db.close()
        }
    }

    /** A v86 file: today's schema (from Room) + what v86 had on top of it. */
    private fun createVersion86() {
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
            db.execSQL("ALTER TABLE issues ADD COLUMN pr_url TEXT")
            db.execSQL("ALTER TABLE issues ADD COLUMN branch TEXT")
            db.execSQL("ALTER TABLE actions ADD COLUMN repository_id TEXT")
            db.execSQL("ALTER TABLE actions ADD COLUMN triggers TEXT")
            db.execSQL("CREATE TABLE coding_sessions (id TEXT NOT NULL PRIMARY KEY, team_id TEXT NOT NULL)")
            db.execSQL("CREATE TABLE threads (id TEXT NOT NULL PRIMARY KEY, team_id TEXT NOT NULL)")
            db.execSQL("CREATE TABLE thread_items (id TEXT NOT NULL PRIMARY KEY, thread_id TEXT NOT NULL)")
            db.execSQL(
                "INSERT INTO issues (id, board_id, number, identifier, title, status, priority, " +
                    "sort_order, pr_url, branch, created_at, updated_at) VALUES ('i1', 'b1', 1, " +
                    "'ACME-1', 'Fix it', 'backlog', 'none', 1.0, 'https://github.com/acme/app/pull/7', " +
                    "'exp/ACME-1', 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO actions (id, team_id, name, sort_order, repository_id, triggers, " +
                    "created_at, updated_at) VALUES ('a1', 't1', 'Nightly', 1.0, 'r1', '[]', 'c', 'u')",
            )
            db.execSQL("INSERT INTO coding_sessions (id, team_id) VALUES ('s1', 't1')")
            db.execSQL("INSERT INTO threads (id, team_id) VALUES ('th1', 't1')")
            for (shape in listOf("issues", "actions", "coding_sessions", "threads", "thread_items", "_repair:x:threads")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 86
        }
    }

    private companion object {
        const val SCHEMA_DB = "exp1291-schema-v87.db"
        const val TEST_DB = "exp1291-migration-v86.db"
    }
}
