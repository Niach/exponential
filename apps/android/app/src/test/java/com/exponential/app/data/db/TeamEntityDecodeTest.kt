package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Wire-decode vectors for the teams shape row. EXP-630 added
 * `estimation_type` (the estimate scale) to the allowlist: a row that carries
 * it must land it, and a row from an older server that does not must still
 * decode (as "no scale" — estimates off) — a required field missing on the
 * wire drops the row forever.
 */
class TeamEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun row(extra: String) = """
        {
          "id": "team-1",
          "name": "Acme",
          "slug": "acme",
          "helpdesk_enabled": false$extra,
          "created_at": "2026-09-15 10:00:00+00",
          "updated_at": "2026-09-15 10:00:00+00"
        }
    """.trimIndent()

    @Test
    fun `a row carrying estimation_type lands the scale`() {
        val entity = json.decodeFromString(
            TeamEntity.serializer(),
            row(",\n  \"estimation_type\": \"tshirt\""),
        )
        assertEquals("tshirt", entity.estimationType)
        assertEquals("acme", entity.slug)
    }

    @Test
    fun `a row without estimation_type still decodes`() {
        assertNull(json.decodeFromString(TeamEntity.serializer(), row("")).estimationType)
    }
}
