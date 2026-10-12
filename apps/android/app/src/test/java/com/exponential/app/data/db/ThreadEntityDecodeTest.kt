package com.exponential.app.data.db

import com.exponential.app.data.api.ThreadWithItems
import com.exponential.app.domain.DomainContract
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Wire-decode vectors for the EXP-1284 threads + thread_items shape rows:
 * every column the `/api/shapes/threads` and `/api/shapes/thread-items`
 * allowlists carry, their camelCase tRPC twins, the jsonb Guide kept as raw
 * text, columns this build does not know, and explicit nulls. A required
 * field missing on the wire silently drops the row forever, so every column
 * but the PK must default.
 */
class ThreadEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `the snake_case threads row decodes in full, the Guide as raw jsonb text`() {
        val row = """
            {
              "id": "thread-1",
              "team_id": "team-1",
              "title": "Fix the login flow",
              "created_by": "user-1",
              "results": [{"topic":"Summary","text":"Done.","files":["a.kt"]}],
              "created_at": "2026-10-12 10:00:00+00",
              "updated_at": "2026-10-12 11:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(ThreadEntity.serializer(), row)
        assertEquals("thread-1", entity.id)
        assertEquals("team-1", entity.teamId)
        assertEquals("Fix the login flow", entity.title)
        assertEquals("user-1", entity.createdBy)
        val results = entity.results!!
        assertTrue(results.startsWith("["))
        assertTrue(results.contains("\"topic\":\"Summary\""))
        assertEquals("2026-10-12 11:00:00+00", entity.updatedAt)
    }

    @Test
    fun `a threads row with nulls and unknown columns keeps its defaults`() {
        val row = """
            {
              "id": "thread-2",
              "team_id": "team-1",
              "title": null,
              "created_by": null,
              "results": null,
              "pinned_at": "2026-10-12 10:00:00+00",
              "some_future_jsonb": {"a": 1}
            }
        """.trimIndent()
        val entity = json.decodeFromString(ThreadEntity.serializer(), row)
        assertEquals("thread-2", entity.id)
        assertNull(entity.title)
        assertNull(entity.createdBy)
        assertNull(entity.results)
        assertEquals("", entity.createdAt)

        // The minimum a row can arrive as: nothing but the PK survives.
        val bare = json.decodeFromString(ThreadEntity.serializer(), """{"id":"thread-3"}""")
        assertEquals("", bare.teamId)
        assertNull(bare.title)
        assertNull(bare.results)
    }

    @Test
    fun `an issue item decodes in full`() {
        val row = """
            {
              "id": "item-1",
              "thread_id": "thread-1",
              "kind": "issue",
              "issue_id": "issue-1",
              "session_id": null,
              "team_id": "team-1",
              "board_id": "board-1",
              "added_by": "user-1",
              "added_by_session_id": null,
              "source": "user",
              "sort_order": 1,
              "created_at": "2026-10-12 10:00:00+00",
              "updated_at": "2026-10-12 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(ThreadItemEntity.serializer(), row)
        assertEquals("thread-1", entity.threadId)
        assertEquals(DomainContract.threadItemKindIssue, entity.kind)
        assertEquals("issue-1", entity.issueId)
        assertNull(entity.sessionId)
        assertEquals("team-1", entity.teamId)
        assertEquals("board-1", entity.boardId)
        assertEquals("user-1", entity.addedBy)
        assertNull(entity.addedBySessionId)
        assertEquals(DomainContract.threadItemSourceUser, entity.source)
        assertEquals(1.0, entity.sortOrder, 0.0)
    }

    @Test
    fun `an agent-linked run item with a fractional order and unknown columns decodes`() {
        val row = """
            {
              "id": "item-2",
              "thread_id": "thread-1",
              "kind": "run",
              "session_id": "session-1",
              "team_id": "team-1",
              "added_by": "user-1",
              "added_by_session_id": "session-0",
              "source": "agent",
              "sort_order": 2.5,
              "board_deleted_at": null,
              "some_future_column": "x"
            }
        """.trimIndent()
        val entity = json.decodeFromString(ThreadItemEntity.serializer(), row)
        assertEquals(DomainContract.threadItemKindRun, entity.kind)
        assertNull(entity.issueId)
        assertEquals("session-1", entity.sessionId)
        // Run items carry no board.
        assertNull(entity.boardId)
        assertEquals("session-0", entity.addedBySessionId)
        assertEquals(DomainContract.threadItemSourceAgent, entity.source)
        assertEquals(2.5, entity.sortOrder, 0.0)
    }

    @Test
    fun `a bare item keeps every optional column defaulted`() {
        val entity = json.decodeFromString(ThreadItemEntity.serializer(), """{"id":"item-3"}""")
        assertEquals("item-3", entity.id)
        assertEquals("", entity.threadId)
        assertEquals("", entity.kind)
        assertNull(entity.issueId)
        assertNull(entity.sessionId)
        assertNull(entity.boardId)
        assertNull(entity.addedBy)
        assertEquals(DomainContract.threadItemSourceUser, entity.source)
        assertEquals(0.0, entity.sortOrder, 0.0)
    }

    @Test
    fun `the camelCase threads get payload decodes too`() {
        val payload = """
            {
              "thread": {
                "id": "thread-4",
                "teamId": "team-1",
                "title": null,
                "createdBy": "user-1",
                "results": [],
                "createdAt": "2026-10-12T10:00:00.000Z",
                "updatedAt": "2026-10-12T10:05:00.000Z"
              },
              "items": [
                {
                  "id": "item-4",
                  "threadId": "thread-4",
                  "kind": "issue",
                  "issueId": "issue-9",
                  "sessionId": null,
                  "teamId": "team-1",
                  "boardId": "board-2",
                  "boardDeletedAt": null,
                  "boardArchivedAt": null,
                  "addedBy": "user-1",
                  "addedBySessionId": "session-7",
                  "source": "agent",
                  "sortOrder": 3,
                  "createdAt": "2026-10-12T10:00:00.000Z",
                  "updatedAt": "2026-10-12T10:00:00.000Z"
                }
              ],
              "txid": 12345
            }
        """.trimIndent()
        val decoded = json.decodeFromString(ThreadWithItems.serializer(), payload)
        assertEquals("team-1", decoded.thread.teamId)
        assertEquals("user-1", decoded.thread.createdBy)
        assertEquals("[]", decoded.thread.results)
        assertEquals("2026-10-12T10:05:00.000Z", decoded.thread.updatedAt)
        val item = decoded.items.single()
        assertEquals("thread-4", item.threadId)
        assertEquals("issue-9", item.issueId)
        assertEquals("board-2", item.boardId)
        assertEquals("session-7", item.addedBySessionId)
        assertEquals(3.0, item.sortOrder, 0.0)
    }
}
