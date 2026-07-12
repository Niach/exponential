package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Booleans on synced rows arrive in three wire forms: native JSON booleans
 * (tRPC), "true"/"false" strings (Electric), and PostgreSQL text "t"/"f"
 * (Electric, observed on staging — EXP-61: an unparseable "f" silently
 * dropped the whole issue_subscribers row).
 */
class PgBoolTest {

    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    private fun subscriber(unsubscribed: String) = json.decodeFromString(
        IssueSubscriberEntity.serializer(),
        """
        {"id":"s1","issue_id":"i1","workspace_id":"w1","source":"manual",
         "unsubscribed":$unsubscribed,
         "created_at":"2026-07-12 00:00:00+00","updated_at":"2026-07-12 00:00:00+00"}
        """.trimIndent(),
    )

    @Test
    fun decodesPostgresTextBooleans() {
        assertFalse(subscriber("\"f\"").unsubscribed)
        assertTrue(subscriber("\"t\"").unsubscribed)
    }

    @Test
    fun decodesStringAndNativeBooleans() {
        assertTrue(subscriber("\"true\"").unsubscribed)
        assertFalse(subscriber("\"false\"").unsubscribed)
        assertTrue(subscriber("true").unsubscribed)
        assertFalse(subscriber("false").unsubscribed)
    }

    @Test
    fun encodesAsNativeBoolean() {
        val row = subscriber("\"t\"")
        val encoded = json.encodeToString(IssueSubscriberEntity.serializer(), row)
        assertTrue(encoded, encoded.contains("\"unsubscribed\":true"))
    }
}
