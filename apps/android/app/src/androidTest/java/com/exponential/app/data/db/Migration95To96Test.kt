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
 * NEW-8: [MIGRATION_95_96] on a real version-95 database, built from today's
 * schema with v95's extras put back: the `actions` table, `pins.action_id`,
 * the legacy session columns and `session_issues.assigned_session_id`. Room
 * opens it with the migration and NO destructive fallback, so every rebuilt
 * table is validated against its entity. Sessions, session issues and the
 * issue pin survive; the actions table, its offsets and the action pin go;
 * every other offset stays.
 */
@RunWith(AndroidJUnit4::class)
class Migration95To96Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun dropsActionsAndTheLegacySessionColumns() {
        createVersion95()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_95_96)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(96, sql.version)

            val session = runBlocking { db.sessionDao().getById("s1") }!!
            assertEquals("Fix it", session.title)
            assertEquals("agent-1", session.agentId)
            val links = runBlocking { db.sessionIssueDao().observeByRoot("s1").first() }
            assertEquals(listOf("i1"), links.map { it.issueId })

            sql.query("SELECT name FROM sqlite_master WHERE type = 'table'").use {
                val tables = ArrayList<String>()
                while (it.moveToNext()) tables.add(it.getString(0))
                assertFalse("actions" in tables)
            }
            sql.query("SELECT id FROM pins").use {
                val ids = ArrayList<String>()
                while (it.moveToNext()) ids.add(it.getString(0))
                assertEquals(listOf("p-issue"), ids)
            }
            val offsets = db.electricOffsetDao()
            for (dropped in listOf("actions", "_repair:x:actions")) {
                assertNull(runBlocking { offsets.get(dropped) })
            }
            for (kept in listOf("sessions", "session_issues", "pins")) {
                val row = runBlocking { offsets.get(kept) }!!
                assertEquals("h", row.handle)
                assertTrue(row.isLive)
            }
        } finally {
            db.close()
        }
    }

    /** A v95 file: today's schema (from Room) + what v95 had on top of it. */
    private fun createVersion95() {
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
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `actions` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                    "`name` TEXT NOT NULL, `description` TEXT, `icon` TEXT, `inputs` TEXT, " +
                    "`prompt_placeholder` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, " +
                    "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL("CREATE INDEX IF NOT EXISTS `index_actions_team_id` ON `actions` (`team_id`)")
            db.execSQL("ALTER TABLE pins ADD COLUMN action_id TEXT")
            db.execSQL("CREATE INDEX IF NOT EXISTS `index_pins_action_id` ON `pins` (`action_id`)")
            for (column in LEGACY_SESSION_COLUMNS) db.execSQL("ALTER TABLE sessions ADD COLUMN $column")
            db.execSQL("ALTER TABLE session_issues ADD COLUMN assigned_session_id TEXT")

            db.execSQL(
                "INSERT INTO actions (id, team_id, name, sort_order, created_at, updated_at) " +
                    "VALUES ('a1', 't1', 'Nightly', 1.0, 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO sessions (id, team_id, root_id, user_id, device_id, plan_mode, computer_use, " +
                    "ultracode, status, busy, needs_input, shared, title, created_at, updated_at, agent_id, " +
                    "kind, dispatch, caption, aside, move_to_device_id, name, sort_order) VALUES ('s1', 't1', " +
                    "'s1', 'u1', 'mac', 0, 0, 0, 'idle', 0, 0, 0, 'Fix it', 'c', 'u', 'agent-1', 'agent', 1, " +
                    "'Reading', 0, 'studio', 'Scout', 2.0)",
            )
            db.execSQL(
                "INSERT INTO session_issues (id, session_id, issue_id, team_id, source, sort_order, " +
                    "created_at, updated_at, assigned_session_id) VALUES ('si1', 's1', 'i1', 't1', 'user', " +
                    "1.0, 'c', 'u', 's2')",
            )
            db.execSQL(
                "INSERT INTO pins (id, user_id, team_id, kind, issue_id, sort_order, created_at, updated_at) " +
                    "VALUES ('p-issue', 'u1', 't1', 'issue', 'i1', 1.0, 'c', 'u')",
            )
            db.execSQL(
                "INSERT INTO pins (id, user_id, team_id, kind, action_id, sort_order, created_at, updated_at) " +
                    "VALUES ('p-action', 'u1', 't1', 'action', 'a1', 2.0, 'c', 'u')",
            )
            for (shape in listOf("sessions", "session_issues", "pins", "actions", "_repair:x:actions")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 95
        }
    }

    private companion object {
        const val SCHEMA_DB = "new8-schema-v96.db"
        const val TEST_DB = "new8-migration-v95.db"

        val LEGACY_SESSION_COLUMNS = listOf(
            "kind TEXT NOT NULL DEFAULT 'thread'",
            "dispatch INTEGER NOT NULL DEFAULT 0",
            "caption TEXT",
            "aside INTEGER NOT NULL DEFAULT 0",
            "move_to_device_id TEXT",
            "action_id TEXT",
            "action_name TEXT",
            "action_inputs TEXT",
            "spawned_by_session_id TEXT",
            "name TEXT",
            "icon TEXT",
            "color TEXT",
            "triggers TEXT",
            "sort_order REAL NOT NULL DEFAULT 0",
        )
    }
}
