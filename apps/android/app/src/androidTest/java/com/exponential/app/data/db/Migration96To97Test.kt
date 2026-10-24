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
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * VAPP-122: [MIGRATION_96_97] on a real version-96 database, built from
 * today's schema without the `app_records` table. Room opens it with the
 * migration (then the idempotent [MIGRATION_97_98] and [MIGRATION_98_99]) and NO destructive fallback, so the new table is validated
 * against [AppRecordEntity]; it starts empty and takes a record, while the
 * apps row and the `apps` offset survive untouched (the `sessions` offset
 * refetches: it is one of [MIGRATION_97_98]'s agent shapes).
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
    fun createsTheAppRecordsTable() {
        createVersion96()

        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_96_97, MIGRATION_97_98, MIGRATION_98_99, MIGRATION_99_100)
            .allowMainThreadQueries()
            .build()
        try {
            val sql = db.openHelper.writableDatabase
            assertEquals(100, sql.version)

            val records = db.appRecordDao()
            assertTrue(runBlocking { records.observeByApp("a1").first() }.isEmpty())
            runBlocking {
                records.upsert(
                    AppRecordEntity(
                        id = "r1", appId = "a1", teamId = "t1", kind = "entry",
                        data = "{\"hours\":3}", createdBy = "u1", createdAt = "c", updatedAt = "u",
                    ),
                )
            }
            assertEquals(listOf("r1"), runBlocking { records.observeByApp("a1").first() }.map { it.id })
            assertEquals(listOf("r1"), runBlocking { records.observeByTeam("t1").first() }.map { it.id })

            val app = runBlocking { db.appDao().observeById("a1").first() }!!
            assertEquals("hours", app.slug)
            // `apps` is no agent shape: its offset survives; `sessions` is one,
            // so [MIGRATION_97_98] made it refetch (#EXP-1383).
            val kept = runBlocking { db.electricOffsetDao().get("apps") }!!
            assertEquals("h", kept.handle)
            assertTrue(kept.isLive)
            val refetched = runBlocking { db.electricOffsetDao().get("sessions") }!!
            assertEquals("", refetched.handle)
            assertFalse(refetched.isLive)
            assertTrue(refetched.needsRefetch)
        } finally {
            db.close()
        }
    }

    /** A v96 file: today's schema (from Room) minus `app_records`. */
    private fun createVersion96() {
        val schema = ArrayList<String>()
        val fresh = Room.databaseBuilder(context, ExponentialDatabase::class.java, SCHEMA_DB).build()
        try {
            fresh.openHelper.writableDatabase.query(
                "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL " +
                    "AND name NOT LIKE 'sqlite_%' AND name != 'android_metadata' " +
                    "AND tbl_name != 'app_records' " +
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
            // v100 (VAPP-120) added `apps.granted_tools`: an older file has none.
            db.execSQL("ALTER TABLE `apps` DROP COLUMN `granted_tools`")
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
            db.version = 96
        }
    }

    private companion object {
        const val SCHEMA_DB = "vapp122-schema-v97.db"
        const val TEST_DB = "vapp122-migration-v96.db"
    }
}
