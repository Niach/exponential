package com.exponential.app.data.db

import com.exponential.app.domain.Chats
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Thread e4a04d22: `conversations.digest` (jsonb) decodes off the shape row
 * into JSON text the Chats list turns back into messages; a row without it
 * (an older server) still decodes with a NULL digest.
 */
class ConversationDigestDecodeTest {

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
            ConversationEntity.serializer(),
            """{"id": "c1", "team_id": "t1", "kind": "session", "session_id": "s1", $stamps,
               "digest": {"first": {"id": "u1", "authorKind": "user", "authorSessionId": null, "body": "Fix it",
                 "card": null, "answeredAt": null, "createdAt": "2026-10-24T10:00:00.000Z"},
                 "preview": null, "waiting": null, "agentAt": "2026-10-24T10:05:00.000Z",
                 "agentTimes": ["2026-10-24T10:05:00.000Z"]}}""",
        )
        val messages = Chats.digestMessages("s1", Chats.parseDigest(row.digest))
        assertEquals(listOf("u1", "digest:s1:0", "digest:s1:agent"), messages.map { it.id })
        assertEquals("Fix it", messages.first().body)
    }

    @Test
    fun `a row without a digest decodes to none`() {
        val row = json.decodeFromString(
            ConversationEntity.serializer(),
            """{"id": "c1", "team_id": "t1", "kind": "session", "session_id": "s1", $stamps}""",
        )
        assertNull(row.digest)
        assertEquals(emptyList<Any>(), Chats.digestMessages("s1", Chats.parseDigest(row.digest)))
    }
}
