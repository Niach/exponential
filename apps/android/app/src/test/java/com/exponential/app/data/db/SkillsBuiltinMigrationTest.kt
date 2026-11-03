package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #VAPP-213: v112 adds `skills.builtin` (NOT NULL DEFAULT 0, matching
 * [SkillEntity]'s Room schema) and refetches ONLY the skills shape, so synced
 * rows re-arrive carrying it.
 */
class SkillsBuiltinMigrationTest {
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
    fun `v112 adds skills builtin and refetches only the skills shape`() {
        assertEquals(111, MIGRATION_111_112.startVersion)
        assertEquals(112, MIGRATION_111_112.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_111_112.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `skills` ADD COLUMN `builtin` INTEGER NOT NULL DEFAULT 0",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'skills'",
            ),
            statements,
        )
    }
}
