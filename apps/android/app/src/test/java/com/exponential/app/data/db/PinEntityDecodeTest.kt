package com.exponential.app.data.db

import com.exponential.app.domain.DomainContract
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Wire-decode vectors for the pins shape row (EXP-778): every column the
 * `/api/shapes/pins` allowlist carries, exactly one target id set, and a
 * `sort_order` that arrives as a Postgres double (integral or fractional).
 * A required field missing on the wire silently drops the row forever, so the
 * optional columns must default.
 */
class PinEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `snake_case shape row decodes an issue pin`() {
        val row = """
            {
              "id": "pin-1",
              "user_id": "user-1",
              "team_id": "team-1",
              "kind": "issue",
              "issue_id": "issue-1",
              "session_id": null,
              "action_id": null,
              "sort_order": 1,
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(PinEntity.serializer(), row)
        assertEquals("pin-1", entity.id)
        assertEquals("user-1", entity.userId)
        assertEquals("team-1", entity.teamId)
        assertEquals(DomainContract.pinKindIssue, entity.kind)
        assertEquals("issue-1", entity.issueId)
        assertNull(entity.sessionId)
        assertTrue(entity.isKept)
        assertEquals(1.0, entity.sortOrder, 0.0)
    }

    @Test
    fun `a fractional sort_order and a session target decode, a legacy action pin is not kept`() {
        val session = """
            {
              "id": "pin-2",
              "user_id": "user-1",
              "team_id": "team-1",
              "kind": "session",
              "session_id": "session-1",
              "sort_order": 2.5,
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val sessionPin = json.decodeFromString(PinEntity.serializer(), session)
        assertEquals(DomainContract.pinKindSession, sessionPin.kind)
        assertEquals("session-1", sessionPin.sessionId)
        assertNull(sessionPin.issueId)
        assertEquals(2.5, sessionPin.sortOrder, 0.0)

        val action = """
            {
              "id": "pin-3",
              "user_id": "user-1",
              "team_id": "team-1",
              "kind": "action",
              "action_id": "action-1",
              "sort_order": 3,
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val actionPin = json.decodeFromString(PinEntity.serializer(), action)
        assertEquals("action", actionPin.kind)
        assertNull(actionPin.sessionId)
        assertNull(actionPin.issueId)
        assertTrue(sessionPin.isKept)
        // NEW-8: actions are gone, so the pins shape never stores it.
        assertFalse(actionPin.isKept)
    }

    @Test
    fun `the camelCase tRPC twin decodes too`() {
        val row = """
            {
              "id": "pin-4",
              "userId": "user-1",
              "teamId": "team-1",
              "kind": "issue",
              "issueId": "issue-9",
              "sortOrder": 4.0,
              "createdAt": "2026-09-11 10:00:00+00",
              "updatedAt": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(PinEntity.serializer(), row)
        assertEquals("issue-9", entity.issueId)
        assertEquals("team-1", entity.teamId)
        assertEquals(4.0, entity.sortOrder, 0.0)
    }

    @Test
    fun `every contract kind decodes`() {
        assertEquals(
            listOf(DomainContract.pinKindIssue, DomainContract.pinKindSession),
            DomainContract.pinKindValues,
        )
    }
}
