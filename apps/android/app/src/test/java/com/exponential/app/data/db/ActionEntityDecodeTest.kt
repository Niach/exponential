package com.exponential.app.data.db

import com.exponential.app.data.api.toActionDto
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Wire-decode vectors for the actions shape row (EXP-825: `prompt_placeholder`,
 * the composer hint). The column must decode when PRESENT, when an explicit
 * null, and when ABSENT (a row synced before the column existed — a required
 * field missing on the wire silently drops the row forever), and the DTO
 * mapping has to carry it through to the composer.
 */
class ActionEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `snake_case shape row decodes the composer hint`() {
        val row = """
            {
              "id": "action-1",
              "team_id": "team-1",
              "repository_id": "repo-1",
              "name": "Release",
              "description": "Cut a release",
              "icon": "rocket",
              "inputs": [{"key": "repo", "label": "Repository", "type": "repo"}],
              "prompt_placeholder": "Scope: which platforms, which version",
              "sort_order": 1.5,
              "created_at": "2026-09-01 10:00:00+00",
              "updated_at": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(ActionEntity.serializer(), row)
        assertEquals("Scope: which platforms, which version", entity.promptPlaceholder)
        val dto = entity.toActionDto(json)
        assertEquals("Scope: which platforms, which version", dto.promptPlaceholder)
        assertEquals(listOf("repo"), dto.inputs?.map { it.key })
    }

    @Test
    fun `an older row decodes with the hint absent or null`() {
        val absent = """
            {
              "id": "action-2",
              "team_id": "team-1",
              "name": "Sweep",
              "sort_order": 2.0,
              "created_at": "2026-09-01 10:00:00+00",
              "updated_at": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(ActionEntity.serializer(), absent)
        assertNull(entity.promptPlaceholder)
        assertNull(entity.toActionDto(json).promptPlaceholder)

        val explicitNull = """
            {
              "id": "action-3",
              "team_id": "team-1",
              "name": "Report",
              "prompt_placeholder": null,
              "sort_order": 3.0,
              "created_at": "2026-09-01 10:00:00+00",
              "updated_at": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        assertNull(json.decodeFromString(ActionEntity.serializer(), explicitNull).promptPlaceholder)
    }

    @Test
    fun `the camelCase tRPC twin decodes too`() {
        val row = """
            {
              "id": "action-4",
              "teamId": "team-1",
              "name": "Release",
              "promptPlaceholder": "Which version?",
              "sortOrder": 1.0,
              "createdAt": "2026-09-01 10:00:00+00",
              "updatedAt": "2026-09-02 10:00:00+00"
            }
        """.trimIndent()
        assertEquals("Which version?", json.decodeFromString(ActionEntity.serializer(), row).promptPlaceholder)
    }
}
