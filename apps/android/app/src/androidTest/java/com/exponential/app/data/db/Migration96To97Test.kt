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
 * #EXP-1383: [MIGRATION_96_97] on a real version-96 file (today's schema
 * minus the eight agent columns). Room opens it with the migration and NO
 * destructive fallback, so every table is validated against its entity; the
 * rows survive with the new columns NULL, the six shapes refetch and every
 * other offset stays.
 */
@RunWith(AndroidJUnit4::class)
class Migration96To97Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun addsTheAgentColumnsAndRefetchesTheirShapes() {
        createVersion96()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_96_97)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(97, sql.version)
            sql.query("SELECT title, assignee_agent_id FROM issues WHERE id = 'i1'").use {
                assertTrue(it.moveToFirst())
                assertEquals("Crash on launch", it.getString(0))
                assertTrue(it.isNull(1))
            }
            val offsets = db.electricOffsetDao()
            for (shape in AGENT_ACTOR_COLUMNS_V97.map { it.first }.distinct()) {
                val row = runBlocking { offsets.get(shape) }!!
                assertEquals(shape, "", row.handle)
                assertEquals("-1", row.offset)
                assertFalse(row.isLive)
                assertTrue(row.needsRefetch)
            }
            val kept = runBlocking { offsets.get("pins") }!!
            assertEquals("h-pins", kept.handle)
            assertTrue(kept.isLive)
            assertNull(runBlocking { offsets.get("actions") })
        } finally {
            db.close()
        }
    }

    private fun createVersion96() {
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
            for ((table, column) in AGENT_ACTOR_COLUMNS_V97) {
                db.execSQL("ALTER TABLE `$table` DROP COLUMN `$column`")
            }
            db.execSQL(
                "INSERT INTO issues (id, board_id, number, identifier, title, status, priority, " +
                    "sort_order, created_at, updated_at) VALUES ('i1', 'b1', 1, 'APP-1', " +
                    "'Crash on launch', 'backlog', 'none', 1.0, 'c', 'u')",
            )
            val shapes = AGENT_ACTOR_COLUMNS_V97.map { it.first }.distinct() + "pins"
            for (shape in shapes) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h-$shape', '7_3', 1, 0)",
                )
            }
            db.version = 96
        }
    }

    private companion object {
        const val SCHEMA_DB = "migration-96-97-schema.db"
        const val TEST_DB = "migration-96-97.db"
    }
}
