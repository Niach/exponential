package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Dispatcher roots: the synced `sessions.dispatch` flag, decoded like `plan_mode`. */
class SessionEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `dispatch decodes from Postgres text booleans and defaults to false`() {
        val root = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id":"s-1","root_id":"s-1","kind":"thread","dispatch":"t","plan_mode":"f"}""",
        )
        assertTrue(root.dispatch)
        assertFalse(root.planMode)
        // A row from a server that predates the column still decodes.
        val older = json.decodeFromString(SessionEntity.serializer(), """{"id":"s-2"}""")
        assertFalse(older.dispatch)
    }
}
