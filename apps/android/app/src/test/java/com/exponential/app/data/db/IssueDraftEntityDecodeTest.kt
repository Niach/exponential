package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Wire-decode vectors for the issue_drafts shape row (EXP-878): every column
 * the `/api/shapes/issue-drafts` allowlist carries, its camelCase tRPC twin,
 * and the Postgres `uuid[]` label list as Electric ships it (`"{a,b}"`).
 * A required field missing on the wire silently drops the row forever, so
 * every optional column must default.
 */
class IssueDraftEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `the snake_case shape row decodes in full`() {
        val row = """
            {
              "id": "draft-1",
              "user_id": "user-1",
              "team_id": "team-1",
              "board_id": "board-1",
              "title": "Something is broken",
              "description": "steps\n\n![shot](/api/attachments/att-1)",
              "status_id": "status-1",
              "priority": "high",
              "assignee_id": "user-2",
              "label_ids": "{label-a,label-b}",
              "due_date": "2026-10-01",
              "created_at": "2026-09-14 10:00:00+00",
              "updated_at": "2026-09-14 11:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(IssueDraftEntity.serializer(), row)
        assertEquals("draft-1", entity.id)
        assertEquals("user-1", entity.userId)
        assertEquals("team-1", entity.teamId)
        assertEquals("board-1", entity.boardId)
        assertEquals("Something is broken", entity.title)
        assertTrue(entity.description.contains("/api/attachments/att-1"))
        assertEquals("status-1", entity.statusId)
        assertEquals("high", entity.priority)
        assertEquals("user-2", entity.assigneeId)
        assertEquals(listOf("label-a", "label-b"), entity.labelIds)
        assertEquals("2026-10-01", entity.dueDate)
        assertEquals("2026-09-14 11:00:00+00", entity.updatedAt)
    }

    @Test
    fun `a partial row keeps every optional column defaulted`() {
        // The minimum a row can arrive as: nothing but the PK survives.
        val entity = json.decodeFromString(IssueDraftEntity.serializer(), """{"id":"draft-2"}""")
        assertEquals("draft-2", entity.id)
        assertEquals("", entity.title)
        assertEquals("", entity.description)
        assertEquals("", entity.boardId)
        assertNull(entity.statusId)
        assertNull(entity.assigneeId)
        assertNull(entity.dueDate)
        // A null status_id means the team's Backlog builtin, not a dropped row.
        assertEquals("none", entity.priority)
        assertEquals(emptyList<String>(), entity.labelIds)
    }

    @Test
    fun `an empty label array and explicit nulls decode`() {
        val row = """
            {
              "id": "draft-3",
              "user_id": "user-1",
              "team_id": "team-1",
              "board_id": "board-1",
              "title": "",
              "description": "just a body",
              "status_id": null,
              "priority": "none",
              "assignee_id": null,
              "label_ids": "{}",
              "due_date": null,
              "created_at": "2026-09-14 10:00:00+00",
              "updated_at": "2026-09-14 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(IssueDraftEntity.serializer(), row)
        assertNull(entity.statusId)
        assertNull(entity.assigneeId)
        assertNull(entity.dueDate)
        assertEquals(emptyList<String>(), entity.labelIds)
        assertEquals("just a body", entity.description)
    }

    @Test
    fun `the camelCase tRPC twin decodes too`() {
        val row = """
            {
              "id": "draft-4",
              "userId": "user-1",
              "teamId": "team-1",
              "boardId": "board-9",
              "title": "From the mutation",
              "description": "",
              "statusId": "status-7",
              "priority": "urgent",
              "assigneeId": "user-3",
              "labelIds": ["label-a"],
              "dueDate": "2026-12-24",
              "createdAt": "2026-09-14 10:00:00+00",
              "updatedAt": "2026-09-14 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(IssueDraftEntity.serializer(), row)
        assertEquals("board-9", entity.boardId)
        assertEquals("status-7", entity.statusId)
        assertEquals("urgent", entity.priority)
        assertEquals("user-3", entity.assigneeId)
        assertEquals(listOf("label-a"), entity.labelIds)
        assertEquals("2026-12-24", entity.dueDate)
    }
}
