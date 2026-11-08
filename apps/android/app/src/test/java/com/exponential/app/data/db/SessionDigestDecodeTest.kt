package com.exponential.app.data.db

import com.exponential.app.domain.Chats
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Thread e4a04d22 / #EXP-1730: `sessions.digest` (jsonb) decodes off the shape
 * row into JSON text the Chats list turns back into messages; a row without
 * it (an older server) still decodes with a NULL digest.
 */
class SessionDigestDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val stamps = """"created_at": "2026-10-24 10:00:00+00", "updated_at": "2026-10-24 10:00:00+00""""

    @Test
    fun `the digest decodes and reads back as messages`() {
        val row = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id": "s1", "team_id": "t1", "root_id": "s1", $stamps,
               "digest": {"first": {"id": "u1", "authorKind": "user", "authorSessionId": null, "body": "Fix it",
                 "card": null, "answeredAt": null, "createdAt": "2026-10-24T10:00:00.000Z"},
                 "preview": null, "waiting": null, "agentAt": "2026-10-24T10:05:00.000Z",
                 "agentTimes": ["2026-10-24T10:05:00.000Z"]}}""",
        )
        val messages = Chats.digestMessages(row.id, Chats.parseDigest(row.digest))
        assertEquals(listOf("u1", "digest:s1:0", "digest:s1:agent"), messages.map { it.id })
        assertEquals("Fix it", messages.first().body)
    }

    @Test
    fun `a row without a digest decodes to none`() {
        val row = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id": "s1", "team_id": "t1", "root_id": "s1", $stamps}""",
        )
        assertNull(row.digest)
        assertEquals(emptyList<Any>(), Chats.digestMessages(row.id, Chats.parseDigest(row.digest)))
    }
}
