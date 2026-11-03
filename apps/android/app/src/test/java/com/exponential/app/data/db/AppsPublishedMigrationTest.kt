package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * eApps V1 (#VAPP-210): v111 adds `apps.published_version` + `draft_package`
 * (both nullable, matching [AppEntity]'s Room schema) and refetches ONLY the
 * apps shape, so synced rows re-arrive carrying them.
 */
class AppsPublishedMigrationTest {
    /** A database that records every `execSQL` (nothing else is called). */
    private fun recording(statements: MutableList<String>): SupportSQLiteDatabase =
        Proxy.newProxyInstance(
            SupportSQLiteDatabase::class.java.classLoader,
            arrayOf(SupportSQLiteDatabase::class.java),
        ) { _, method, args ->
            if (method.name == "execSQL") {
                statements += args!![0] as String
                null
            } else {
                error("unexpected ${method.name}")
            }
        } as SupportSQLiteDatabase

    @Test
    fun `v111 adds the two apps columns and refetches only the apps shape`() {
        assertEquals(110, MIGRATION_110_111.startVersion)
        assertEquals(111, MIGRATION_110_111.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_110_111.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `apps` ADD COLUMN `published_version` INTEGER",
                "ALTER TABLE `apps` ADD COLUMN `draft_package` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'apps'",
            ),
            statements,
        )
    }
}
