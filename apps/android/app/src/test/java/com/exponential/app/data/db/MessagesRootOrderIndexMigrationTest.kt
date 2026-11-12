package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Large chats: v123 swaps `messages (root_id)` for `(root_id, created_at, id)`
 * (a chat's newest page and its result cards = range reads in transcript
 * order); local only, nothing refetches. The index name is the one Room
 * derives from [MessageEntity]'s `@Index`, or the open fails validation.
 */
class MessagesRootOrderIndexMigrationTest {
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
    fun `v123 replaces the root_id index with root_id created_at id and refetches nothing`() {
        assertEquals(122, MIGRATION_122_123.startVersion)
        assertEquals(123, MIGRATION_122_123.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_122_123.migrate(recording(statements))
        assertEquals(
            listOf(
                "DROP INDEX IF EXISTS `index_messages_root_id`",
                "CREATE INDEX IF NOT EXISTS `index_messages_root_id_created_at_id` " +
                    "ON `messages` (`root_id`, `created_at`, `id`)",
            ),
            statements,
        )
        assertTrue(statements.none { it.contains("electric_offsets") })
    }
}
