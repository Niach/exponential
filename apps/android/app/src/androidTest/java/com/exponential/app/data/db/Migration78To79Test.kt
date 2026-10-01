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
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * SLOP-3: [MIGRATION_78_79] on a real version-78 database. The project has no
 * exported Room schemas and no JVM SQLite (no Robolectric), so this runs on a
 * device: the v78 file is built by raw SQL from the CURRENT schema Room itself
 * creates, plus what v78 had on top of it (the three workflow tables, the
 * dropped columns, their Electric offsets). Room then opens it with the
 * migration and NO destructive fallback, so a schema the generated validation
 * rejects fails the test instead of silently wiping.
 */
@RunWith(AndroidJUnit4::class)
class Migration78To79Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun keepsSessionsIssuesAndTheOtherShapesOffsets() {
        createVersion78()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_78_79)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(79, sql.version)

            sql.query("SELECT id, pr_url, pr_state, batch_issue_ids, pending_question FROM coding_sessions").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("s1", it.getString(0))
                assertEquals("https://github.com/acme/app/pull/7", it.getString(1))
                assertEquals("open", it.getString(2))
                assertEquals("{a,b}", it.getString(3))
                assertEquals("{\"question\":\"q\"}", it.getString(4))
            }
            sql.query("SELECT id, branch, pr_url FROM issues").use {
                assertEquals(1, it.count)
                it.moveToFirst()
                assertEquals("i1", it.getString(0))
                assertEquals("exp/ACME-1", it.getString(1))
                assertEquals("https://github.com/acme/app/pull/7", it.getString(2))
            }
            sql.query("SELECT shape, handle, `offset` FROM electric_offsets ORDER BY shape").use {
                val shapes = ArrayList<String>()
                while (it.moveToNext()) shapes.add(it.getString(0))
                assertEquals(listOf("_repair:x:issues", "coding_sessions", "issues"), shapes)
            }
            sql.query("SELECT name FROM sqlite_master WHERE type = 'table'").use {
                val tables = ArrayList<String>()
                while (it.moveToNext()) tables.add(it.getString(0))
                for (dropped in DROPPED_WORKFLOW_TABLES) assertFalse(dropped in tables)
            }
            sql.query("PRAGMA table_info(`coding_sessions`)").use {
                val columns = ArrayList<String>()
                while (it.moveToNext()) columns.add(it.getString(it.getColumnIndexOrThrow("name")))
                assertFalse("workflow_id" in columns)
                assertFalse("workflow_role" in columns)
            }
            // The DAO reads the migrated rows like synced ones.
            assertEquals("h", runBlocking { db.electricOffsetDao().get("issues") }?.handle)
        } finally {
            db.close()
        }
    }

    /** A v78 file: today's schema (from Room) + what v78 had on top of it. */
    private fun createVersion78() {
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
            db.execSQL("ALTER TABLE issues ADD COLUMN pr_base_branch TEXT")
            db.execSQL("ALTER TABLE coding_sessions ADD COLUMN workflow_id TEXT")
            db.execSQL("ALTER TABLE coding_sessions ADD COLUMN workflow_node_id TEXT")
            db.execSQL("ALTER TABLE coding_sessions ADD COLUMN workflow_role TEXT")
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `workflows` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                    "`name` TEXT NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `workflow_nodes` (`id` TEXT NOT NULL, " +
                    "`workflow_id` TEXT NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `workflow_events` (`id` TEXT NOT NULL, " +
                    "`workflow_id` TEXT NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL("INSERT INTO workflows VALUES ('w1', 't1', 'Checkout')")
            db.execSQL("INSERT INTO workflow_nodes VALUES ('n1', 'w1')")

            db.execSQL(
                "INSERT INTO issues (id, board_id, number, identifier, title, status, priority, " +
                    "sort_order, branch, pr_url, pr_state, pr_base_branch, created_at, updated_at) VALUES " +
                    "('i1', 'b1', 1, 'ACME-1', 'Fix it', 'in_review', 'none', 1.0, 'exp/ACME-1', " +
                    "'https://github.com/acme/app/pull/7', 'open', 'exp/ACME-0', 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO coding_sessions (id, team_id, user_id, status, needs_input, agent_busy, " +
                    "batch_issue_ids, pending_question, pr_url, pr_state, workflow_id, workflow_role, " +
                    "started_at, created_at, updated_at) VALUES ('s1', 't1', 'u1', 'in_review', 0, 0, " +
                    "'{a,b}', '{\"question\":\"q\"}', 'https://github.com/acme/app/pull/7', 'open', " +
                    "'w1', 'author', 's', 'c', 'u')",
            )
            for (shape in listOf(
                "issues", "coding_sessions", "workflows", "workflow_nodes", "workflow_events",
                "_repair:x:workflows", "_repair:x:issues",
            )) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 78
        }
    }

    private companion object {
        const val SCHEMA_DB = "slop3-schema-v79.db"
        const val TEST_DB = "slop3-migration-v78.db"
    }
}
