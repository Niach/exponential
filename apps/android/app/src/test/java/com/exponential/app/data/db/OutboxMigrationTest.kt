package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Test

/** #VAPP-235: v115 adds the LOCAL `app_record_outbox` table (no shape refetches). */
class OutboxMigrationTest {
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
    fun `v115 creates the outbox table and refetches nothing`() {
        assertEquals(114, MIGRATION_114_115.startVersion)
        assertEquals(115, MIGRATION_114_115.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_114_115.migrate(recording(statements))
        assertEquals(2, statements.size)
        assert(statements[0].startsWith("CREATE TABLE IF NOT EXISTS `app_record_outbox`"))
        assert(statements[1].contains("index_app_record_outbox_app_id"))
        assert(statements.none { it.contains("electric_offsets") })
    }
}
