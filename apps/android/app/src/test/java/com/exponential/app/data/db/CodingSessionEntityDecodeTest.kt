package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-637: wire-decode vectors for the coding_sessions shape rows, now that
 * the shape carries the agent's own close-out (`summary`, `outcome`),
 * `ended_by` and `resumed_from_id`.
 *
 * Every one of them must decode when ABSENT — an old server, or simply a row
 * that ended before the columns existed. A required field missing on the wire
 * silently drops the row forever (the attachments.uploader_id lesson), which
 * on this shape would blank the whole Agents tab.
 */
class CodingSessionEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `full snake_case row decodes the EXP-637 columns`() {
        val row = """
            {
              "id": "sess-1",
              "issue_id": null,
              "team_id": "team-1",
              "user_id": "user-1",
              "device_label": "buildbox",
              "device_id": "dev-1",
              "status": "ended",
              "branch": "exp/chat-1a2b3c4d",
              "summary": "Refreshed the shots and pushed a PR.",
              "outcome": "done",
              "ended_by": "agent",
              "resumed_from_id": "sess-0",
              "needs_input": "f",
              "action_name": "Refresh screenshots",
              "started_reason": "schedule",
              "started_at": "2026-08-11 10:00:00+00",
              "ended_at": "2026-08-11 10:20:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:20:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals("Refreshed the shots and pushed a PR.", entity.summary)
        assertEquals("done", entity.outcome)
        assertEquals("agent", entity.endedBy)
        assertEquals("sess-0", entity.resumedFromId)
        assertEquals("2026-08-11 10:20:00+00", entity.endedAt)
    }

    @Test
    fun `camelCase (tRPC-shaped) keys decode via JsonNames`() {
        val row = """
            {
              "id": "sess-1",
              "teamId": "team-1",
              "userId": "user-1",
              "status": "ended",
              "outcome": "blocked",
              "endedBy": "agent",
              "resumedFromId": "sess-0",
              "startedAt": "2026-08-11T10:00:00Z",
              "createdAt": "2026-08-11T10:00:00Z",
              "updatedAt": "2026-08-11T10:00:00Z"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals("blocked", entity.outcome)
        assertEquals("agent", entity.endedBy)
        assertEquals("sess-0", entity.resumedFromId)
    }

    /** The case that matters: an older server (or an older row) sends none of
     * the four keys. The row must still decode, with everything null. */
    @Test
    fun `a row without the EXP-637 keys still decodes`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "running",
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertNull(entity.summary)
        assertNull(entity.outcome)
        assertNull(entity.endedBy)
        assertNull(entity.resumedFromId)
        // The pre-existing optional columns keep their defaults too.
        assertNull(entity.issueId)
        assertNull(entity.branch)
        assertEquals("running", entity.status)
    }

    @Test
    fun `explicit nulls decode as absent`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "ended",
              "summary": null,
              "outcome": null,
              "ended_by": null,
              "resumed_from_id": null,
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertNull(entity.summary)
        assertNull(entity.outcome)
        assertNull(entity.endedBy)
        assertNull(entity.resumedFromId)
    }

    /** A summary is free-form GFM: newlines, quotes and unicode ride through
     * as written (mobile renders it as plain text). */
    @Test
    fun `a multi-line summary survives the round trip`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "ended",
              "summary": "Line one.\n\n- bullet \"quoted\"\n- ✅ done",
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertTrue(entity.summary!!.contains("\n\n- bullet \"quoted\""))
        assertTrue(entity.summary!!.endsWith("✅ done"))
    }
}
