package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1682: `messages.attachment_ids` (a nullable uuid[]) syncs as its wire
 * text and reads as ids; v117 adds the column and refetches ONLY the messages
 * shape.
 */
class MessageAttachmentsTest {
    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val a = "11111111-1111-4111-8111-111111111111"
    private val b = "22222222-2222-4222-8222-222222222222"

    private fun decode(cell: String): MessageEntity =
        json.decodeFromString(MessageEntity.serializer(), """{"id":"m1","body":"hi","attachment_ids":$cell}""")

    @Test
    fun `an Electric text literal reads as ids`() {
        val row = decode("\"{$a,$b}\"")
        assertEquals("{$a,$b}", row.attachmentIds)
        assertEquals(listOf(a, b), row.attachmentIdList)
    }

    @Test
    fun `a native JSON array reads as ids`() {
        assertEquals(listOf(a), decode("[\"$a\"]").attachmentIdList)
    }

    @Test
    fun `null, empty and absent read as none`() {
        assertNull(decode("null").attachmentIds)
        assertEquals(emptyList<String>(), decode("null").attachmentIdList)
        assertEquals(emptyList<String>(), decode("\"{}\"").attachmentIdList)
        val absent = json.decodeFromString(MessageEntity.serializer(), """{"id":"m1"}""")
        assertEquals(emptyList<String>(), absent.attachmentIdList)
    }

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
    fun `v117 adds messages attachment_ids and refetches only the messages shape`() {
        assertEquals(116, MIGRATION_116_117.startVersion)
        assertEquals(117, MIGRATION_116_117.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_116_117.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `messages` ADD COLUMN `attachment_ids` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'messages'",
            ),
            statements,
        )
    }
}
