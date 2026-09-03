package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Wire-decode vectors for the boards shape row. Every optional column must
 * decode when ABSENT (a required field missing on the wire silently drops the
 * row forever — the attachments.uploader_id lesson), and the tRPC camelCase
 * twin has to decode too: `boards.create` returns the created row and the
 * create path upserts it into Room ahead of Electric (EXP-46).
 */
class BoardEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `snake_case shape row decodes the board branch`() {
        val row = """
            {
              "id": "board-1",
              "team_id": "team-1",
              "name": "Backend API",
              "slug": "backend-api",
              "prefix": "API",
              "color": "#6366f1",
              "icon": "square-kanban",
              "repository_id": "repo-1",
              "default_branch": "develop",
              "sort_order": 1.5,
              "deleted_at": null,
              "created_at": "2026-09-01 10:00:00+00",
              "updated_at": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(BoardEntity.serializer(), row)
        assertEquals("repo-1", entity.repositoryId)
        assertEquals("develop", entity.defaultBranch)
    }

    @Test
    fun `a board following its repo decodes with the branch absent`() {
        val row = """
            {
              "id": "board-2",
              "team_id": "team-1",
              "name": "Design",
              "slug": "design",
              "prefix": "DES",
              "color": "#6366f1",
              "sort_order": 2.0,
              "created_at": "2026-09-01 10:00:00+00",
              "updated_at": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(BoardEntity.serializer(), row)
        // NULL = follow the backing repo's default branch (EXP-712).
        assertNull(entity.defaultBranch)
        assertNull(entity.repositoryId)
        assertNull(entity.icon)
    }

    @Test
    fun `the tRPC camelCase row decodes too`() {
        val row = """
            {
              "id": "board-3",
              "teamId": "team-1",
              "name": "Mobile",
              "slug": "mobile",
              "prefix": "MOB",
              "color": "#6366f1",
              "repositoryId": "repo-1",
              "defaultBranch": "release/26",
              "sortOrder": 3.0,
              "createdAt": "2026-09-01 10:00:00+00",
              "updatedAt": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(BoardEntity.serializer(), row)
        assertEquals("team-1", entity.teamId)
        assertEquals("release/26", entity.defaultBranch)
    }
}
