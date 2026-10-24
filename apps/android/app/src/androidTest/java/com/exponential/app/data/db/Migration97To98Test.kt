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
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * VAPP-120: [MIGRATION_97_98] on a real version-97 database, built from
 * today's schema with the `apps` table minus `granted_tools`. Room opens it
 * with the migration and NO destructive fallback, so the column is validated
 * against [AppEntity]; the existing app reads "[]" (no grants), a grant
 * round-trips, and every offset survives untouched.
 */
@RunWith(AndroidJUnit4::class)
class Migration97To98Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(SCHEMA_DB)
        context.deleteDatabase(TEST_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun addsTheGrantedToolsColumn() {
        createVersion97()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_97_98)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(98, sql.version)

            val apps = db.appDao()
            val app = runBlocking { apps.observeById("a1").first() }!!
            assertEquals("hours", app.slug)
            assertEquals("[]", app.grantedTools)

            runBlocking { apps.upsert(app.copy(grantedTools = "[\"exponential.issues.update\"]")) }
            assertEquals(
                "[\"exponential.issues.update\"]",
                runBlocking { apps.observeById("a1").first() }!!.grantedTools,
            )

            for (kept in listOf("apps", "sessions")) {
                val row = runBlocking { db.electricOffsetDao().get(kept) }!!
                assertEquals("h", row.handle)
                assertTrue(row.isLive)
            }
        } finally {
            db.close()
        }
    }

    /** A v97 file: today's schema (from Room) with `apps` minus `granted_tools`. */
    private fun createVersion97() {
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
                val v97 = if (statement.startsWith("CREATE TABLE `apps`")) {
                    statement.replace(Regex(""",\s*`granted_tools` TEXT[^,]*"""), "")
                } else {
                    statement
                }
                if (statement.startsWith("CREATE TABLE `apps`")) assertTrue(!v97.contains("granted_tools"))
                db.execSQL(v97)
            }
            db.execSQL(
                "INSERT INTO apps (id, team_id, slug, name, description, created_by, created_at, updated_at) " +
                    "VALUES ('a1', 't1', 'hours', 'Hours', '', 'u1', 'c', 'u')",
            )
            for (shape in listOf("apps", "sessions")) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES ('$shape', 'h', '0_0', 1, 0)",
                )
            }
            db.version = 97
        }
    }

    private companion object {
        const val SCHEMA_DB = "vapp120-schema-v98.db"
        const val TEST_DB = "vapp120-migration-v97.db"
    }
}
