package com.exponential.app.data.db

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * FEED-33: `devices.shared_team_ids` is a Postgres uuid[]. Electric ships it
 * as the text array literal inside a JSON string, tRPC as a native array;
 * everything else must read as EMPTY rather than drop the row.
 */
class PgUuidArraySerializerTest {

    @Serializable
    private data class Holder(
        @Serializable(with = PgUuidArraySerializer::class) val ids: List<String> = emptyList(),
    )

    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    private fun decode(cell: String): List<String> =
        json.decodeFromString(Holder.serializer(), """{"ids":$cell}""").ids

    @Test
    fun `text literal with zero one and two ids`() {
        assertTrue(decode("\"{}\"").isEmpty())
        assertEquals(listOf("7b62ba88-8dda-4166-9b8e-606acb5d1954"), decode("\"{7b62ba88-8dda-4166-9b8e-606acb5d1954}\""))
        assertEquals(
            listOf("7b62ba88-8dda-4166-9b8e-606acb5d1954", "9836918a-8de3-4299-a167-1dc987a99f2b"),
            decode("\"{7b62ba88-8dda-4166-9b8e-606acb5d1954,9836918a-8de3-4299-a167-1dc987a99f2b}\""),
        )
    }

    @Test
    fun `quoted and padded elements are trimmed, empties and duplicates dropped`() {
        assertEquals(listOf("a", "b"), decode("\"{\\\"a\\\", b ,,a}\""))
        assertEquals(listOf("a"), decode("\" { a } \""))
    }

    @Test
    fun `a native JSON array decodes as-is`() {
        assertEquals(listOf("a", "b"), decode("[\"a\",\"b\"]"))
        assertTrue(decode("[]").isEmpty())
        // A JSON array as text (a converter round-trip) also reads.
        assertEquals(listOf("a"), decode("\"[\\\"a\\\"]\""))
    }

    @Test
    fun `null absent and garbage read as empty`() {
        assertTrue(decode("null").isEmpty())
        assertTrue(json.decodeFromString(Holder.serializer(), "{}").ids.isEmpty())
        assertTrue(decode("\"garbage\"").isEmpty())
        assertTrue(decode("42").isEmpty())
        assertTrue(decode("{\"x\":1}").isEmpty())
        assertTrue(decode("\"[not json\"").isEmpty())
    }

    @Test
    fun `room converter round-trips`() {
        val ids = listOf("a", "b")
        assertEquals(ids, StringListConverters.toList(StringListConverters.fromList(ids)))
        assertEquals("[]", StringListConverters.fromList(emptyList()))
        assertTrue(StringListConverters.toList("").isEmpty())
    }

    @Test
    fun `encodes as a JSON array`() {
        assertEquals("""{"ids":["a"]}""", json.encodeToString(Holder.serializer(), Holder(listOf("a"))))
    }
}
