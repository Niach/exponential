package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-637: wire-decode vectors for the coding_sessions shape rows, now that
 * the shape carries the agent's own close-out (`summary`), `ended_by` and
 * `resumed_from_id`. EXP-686 dropped the companion `outcome` column.
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
              "endedBy": "agent",
              "resumedFromId": "sess-0",
              "startedAt": "2026-08-11T10:00:00Z",
              "createdAt": "2026-08-11T10:00:00Z",
              "updatedAt": "2026-08-11T10:00:00Z"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
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
              "ended_by": null,
              "resumed_from_id": null,
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertNull(entity.summary)
        assertNull(entity.endedBy)
        assertNull(entity.resumedFromId)
    }

    /** EXP-686: `outcome` left the shape. A server (or a cached row) that
     * still sends it must not break the decode — ignoreUnknownKeys carries it,
     * and nothing reads it anymore. */
    @Test
    fun `a stray outcome key is ignored`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "ended",
              "summary": "Bumped the deps.",
              "outcome": "done",
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:20:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals("Bumped the deps.", entity.summary)
        assertEquals("ended", entity.status)
    }

    // EXP-484: the agent a run launched with — absent on every pre-EXP-484 row.
    @Test
    fun `agent decodes and defaults null`() {
        fun row(extra: String) = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "running"$extra,
              "started_at": "2026-08-11 10:00:00+00",
              "created_at": "2026-08-11 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        assertEquals(
            "codex",
            json.decodeFromString(CodingSessionEntity.serializer(), row(", \"agent\": \"codex\"")).agent,
        )
        assertNull(json.decodeFromString(CodingSessionEntity.serializer(), row(", \"agent\": null")).agent)
        assertNull(json.decodeFromString(CodingSessionEntity.serializer(), row("")).agent)
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

    // EXP-734: the run's OWN chore PR — pr_url / pr_number / pr_state on the
    // session row (an action or chat run whose PR links no issue).
    @Test
    fun `the EXP-734 pr columns decode from snake_case`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "in_review",
              "action_name": "Refresh screenshots",
              "pr_url": "https://github.com/acme/app/pull/12",
              "pr_number": 12,
              "pr_state": "open",
              "started_at": "2026-09-04 10:00:00+00",
              "created_at": "2026-09-04 10:00:00+00",
              "updated_at": "2026-09-04 10:20:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals("https://github.com/acme/app/pull/12", entity.prUrl)
        assertEquals(12, entity.prNumber)
        assertEquals("open", entity.prState)
    }

    @Test
    fun `the EXP-734 pr columns decode from camelCase too`() {
        val row = """
            {
              "id": "sess-1",
              "teamId": "team-1",
              "userId": "user-1",
              "status": "in_review",
              "prUrl": "https://github.com/acme/app/pull/12",
              "prNumber": 12,
              "prState": "merged",
              "startedAt": "2026-09-04T10:00:00Z",
              "createdAt": "2026-09-04T10:00:00Z",
              "updatedAt": "2026-09-04T10:20:00Z"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals("https://github.com/acme/app/pull/12", entity.prUrl)
        assertEquals(12, entity.prNumber)
        assertEquals("merged", entity.prState)
    }

    /** Postgres TEXT form of an integer column — the desktop decodes every
     *  integer through a tolerant helper for exactly this reason, and a throw
     *  here would DROP the whole row (blanking the Agents tab). */
    @Test
    fun `a string-form pr_number decodes to an int`() {
        val row = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "in_review",
              "pr_number": "12",
              "started_at": "2026-09-04 10:00:00+00",
              "created_at": "2026-09-04 10:00:00+00",
              "updated_at": "2026-09-04 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), row)
        assertEquals(12, entity.prNumber)
    }

    /** An old server, or an issue/batch run: none of the three keys, or all
     *  three explicitly null. Both must decode with everything null. */
    @Test
    fun `absent and null pr columns default to null`() {
        val bare = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "running",
              "started_at": "2026-09-04 10:00:00+00",
              "created_at": "2026-09-04 10:00:00+00",
              "updated_at": "2026-09-04 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(CodingSessionEntity.serializer(), bare)
        assertNull(entity.prUrl)
        assertNull(entity.prNumber)
        assertNull(entity.prState)

        val nulled = """
            {
              "id": "sess-1",
              "team_id": "team-1",
              "user_id": "user-1",
              "status": "running",
              "pr_url": null,
              "pr_number": null,
              "pr_state": null,
              "started_at": "2026-09-04 10:00:00+00",
              "created_at": "2026-09-04 10:00:00+00",
              "updated_at": "2026-09-04 10:00:00+00"
            }
        """.trimIndent()
        val nullEntity = json.decodeFromString(CodingSessionEntity.serializer(), nulled)
        assertNull(nullEntity.prUrl)
        assertNull(nullEntity.prNumber)
        assertNull(nullEntity.prState)
    }
}
