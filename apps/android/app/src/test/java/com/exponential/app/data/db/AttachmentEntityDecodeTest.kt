package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Wire-decode vectors for the attachments shape row after EXP-824: the two
 * media columns (`duration_ms`, `poster_storage_key`) arrive on the snake_case
 * Electric wire and the camelCase REST twin, and BOTH must default when
 * absent — a required field missing on a partial update stalls the shape.
 */
class AttachmentEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `snake_case shape row decodes a video with poster and duration`() {
        val row = """
            {
              "id": "att-1",
              "team_id": "team-1",
              "issue_id": "issue-1",
              "board_id": "board-1",
              "comment_id": null,
              "uploader_id": "user-1",
              "filename": "clip.mp4",
              "content_type": "video/mp4",
              "size_bytes": 1234567,
              "storage_key": "teams/team-1/att-1",
              "url": "/api/attachments/att-1",
              "width": 1280,
              "height": 720,
              "duration_ms": 7345,
              "poster_storage_key": "teams/team-1/att-1.poster",
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(AttachmentEntity.serializer(), row)
        assertEquals("att-1", entity.id)
        assertEquals("video/mp4", entity.contentType)
        assertEquals(1280, entity.width)
        assertEquals(720, entity.height)
        assertEquals(7345L, entity.durationMs)
        assertEquals("teams/team-1/att-1.poster", entity.posterStorageKey)
        assertTrue(entity.hasPoster)
    }

    @Test
    fun `an image row without the media columns still decodes`() {
        val row = """
            {
              "id": "att-2",
              "team_id": "team-1",
              "issue_id": "issue-1",
              "filename": "shot.png",
              "content_type": "image/png",
              "size_bytes": 100,
              "storage_key": "k",
              "url": "/api/attachments/att-2",
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(AttachmentEntity.serializer(), row)
        assertNull(entity.durationMs)
        assertNull(entity.posterStorageKey)
        assertFalse(entity.hasPoster)
        assertNull(entity.width)
    }

    @Test
    fun `explicit nulls on the wire decode as absent`() {
        val row = """
            {
              "id": "att-3",
              "team_id": "team-1",
              "issue_id": "issue-1",
              "filename": "voice.m4a",
              "content_type": "audio/mp4",
              "size_bytes": 100,
              "storage_key": "k",
              "url": "/api/attachments/att-3",
              "width": null,
              "height": null,
              "duration_ms": 61000,
              "poster_storage_key": null,
              "created_at": "2026-09-11 10:00:00+00",
              "updated_at": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(AttachmentEntity.serializer(), row)
        assertEquals(61000L, entity.durationMs)
        assertNull(entity.posterStorageKey)
        assertFalse(entity.hasPoster)
    }

    @Test
    fun `the camelCase REST twin decodes too`() {
        val row = """
            {
              "id": "att-4",
              "teamId": "team-1",
              "issueId": "issue-1",
              "commentId": "comment-1",
              "uploaderId": "user-1",
              "filename": "clip.mp4",
              "contentType": "video/mp4",
              "sizeBytes": 5,
              "storageKey": "k",
              "url": "/api/attachments/att-4",
              "width": 640,
              "height": 360,
              "durationMs": 3000,
              "posterStorageKey": "k.poster",
              "createdAt": "2026-09-11 10:00:00+00",
              "updatedAt": "2026-09-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(AttachmentEntity.serializer(), row)
        assertEquals("comment-1", entity.commentId)
        assertEquals(3000L, entity.durationMs)
        assertEquals("k.poster", entity.posterStorageKey)
        assertTrue(entity.hasPoster)
    }
}
