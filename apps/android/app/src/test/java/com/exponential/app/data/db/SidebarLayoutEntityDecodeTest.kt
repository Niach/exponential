package com.exponential.app.data.db

import com.exponential.app.domain.SidebarFolders
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Wire-decode vectors for the sidebar_layout shape row (#EXP-1832): every
 * column the `/api/shapes/sidebar-layout` allowlist carries, a folder (name,
 * no target) and an item (target, parent), `sort_order` a Postgres double. A
 * required field missing on the wire silently drops the row forever, so the
 * optional columns must default.
 */
class SidebarLayoutEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `snake_case shape rows decode a folder and an item`() {
        val folder = json.decodeFromString(
            SidebarLayoutEntity.serializer(),
            """
            {
              "id": "f-1",
              "user_id": "user-1",
              "team_id": "team-1",
              "section": "board",
              "kind": "folder",
              "target_id": null,
              "parent_id": null,
              "name": "Monitoring",
              "sort_order": 3,
              "created_at": "2026-11-14 10:00:00+00",
              "updated_at": "2026-11-14 10:00:00+00"
            }
            """.trimIndent(),
        )
        assertEquals(SidebarFolders.Entry("f-1", "folder", null, null, "Monitoring", 3.0), folder.toEntry())
        assertEquals("team-1", folder.teamId)
        assertEquals(SidebarFolders.SECTION_BOARD, folder.section)

        val item = json.decodeFromString(
            SidebarLayoutEntity.serializer(),
            """
            {
              "id": "e-1",
              "user_id": "user-1",
              "team_id": "team-1",
              "section": "app",
              "kind": "item",
              "target_id": "app-1",
              "parent_id": "f-1",
              "sort_order": 1.5,
              "created_at": "2026-11-14 10:00:00+00",
              "updated_at": "2026-11-14 10:00:00+00"
            }
            """.trimIndent(),
        )
        assertEquals(SidebarFolders.Entry("e-1", "item", "app-1", "f-1", null, 1.5), item.toEntry())
        assertNull(item.name)
        assertEquals(SidebarFolders.SECTION_APP, item.section)
    }

    @Test
    fun `the camelCase tRPC twin decodes too`() {
        val row = json.decodeFromString(
            SidebarLayoutEntity.serializer(),
            """{"id": "e-2", "userId": "user-1", "teamId": "team-1", "section": "agent", "kind": "item", "targetId": "agent-1", "sortOrder": 2}""",
        )
        assertEquals("team-1", row.teamId)
        assertEquals("agent-1", row.targetId)
        assertNull(row.parentId)
        assertEquals(2.0, row.sortOrder, 0.0)
    }
}
