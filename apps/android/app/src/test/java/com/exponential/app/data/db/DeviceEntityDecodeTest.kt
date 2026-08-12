package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-481: wire-decode vectors for the devices/device_worktrees shape rows.
 * Every optional column must decode when ABSENT (a required field missing on
 * the wire silently drops the row forever — the attachments.uploader_id
 * lesson), jsonb columns must survive arriving as objects/arrays (stored as
 * their raw JSON text via JsonAsStringSerializer), and Postgres text booleans
 * ("t"/"f") must parse (PgBool).
 */
class DeviceEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `full snake_case row decodes with jsonb columns kept as text`() {
        val row = """
            {
              "id": "row-1",
              "user_id": "user-1",
              "device_id": "dev-1",
              "label": "buildbox",
              "kind": "server",
              "platform": "linux",
              "version": "0.9.0",
              "agents": ["claude", "codex"],
              "caps": ["actions", "resume", "worktrees", "launch-defaults"],
              "unauthed_agents": ["pi"],
              "launch_defaults": {"defaultAgent": "claude", "agents": {"claude": {"model": "fable"}}},
              "launch_defaults_updated_at": "2026-08-10 10:00:00+00",
              "active_sessions": 2,
              "last_seen_at": "2026-08-11 10:00:00+00",
              "shared_team_id": "team-1",
              "update_requested_at": null,
              "created_at": "2026-08-01 10:00:00+00",
              "updated_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(DeviceEntity.serializer(), row)
        assertEquals("dev-1", entity.deviceId)
        assertEquals("server", entity.kind)
        assertEquals(2, entity.activeSessions)
        // jsonb lands as its raw JSON text, parsed later by DeviceRows.
        assertTrue(entity.agents!!.contains("claude"))
        assertTrue(entity.launchDefaults!!.contains("defaultAgent"))
        assertNull(entity.updateRequestedAt)
    }

    @Test
    fun `camelCase (tRPC-shaped) keys decode via JsonNames`() {
        val row = """
            {
              "id": "row-1",
              "userId": "user-1",
              "deviceId": "dev-1",
              "label": "mac",
              "lastSeenAt": "2026-08-11T10:00:00Z",
              "sharedTeamId": null
            }
        """.trimIndent()
        val entity = json.decodeFromString(DeviceEntity.serializer(), row)
        assertEquals("dev-1", entity.deviceId)
        assertEquals("2026-08-11T10:00:00Z", entity.lastSeenAt)
        assertNull(entity.sharedTeamId)
    }

    @Test
    fun `minimal row decodes — every optional column defaulted`() {
        val entity = json.decodeFromString(
            DeviceEntity.serializer(),
            """{"id": "row-1", "user_id": "u", "device_id": "d"}""",
        )
        assertEquals("", entity.label)
        assertEquals("desktop", entity.kind)
        assertNull(entity.agents)
        assertNull(entity.launchDefaults)
        assertEquals(0, entity.activeSessions)
    }

    @Test
    fun `quoted active_sessions (Postgres text int) decodes`() {
        // Electric delivers row values as text; kotlinx's lenient primitive
        // coercion must absorb the quoted form.
        val entity = json.decodeFromString(
            DeviceEntity.serializer(),
            """{"id": "row-1", "user_id": "u", "device_id": "d", "active_sessions": "3"}""",
        )
        assertEquals(3, entity.activeSessions)
    }

    @Test
    fun `worktree row decodes with Postgres text booleans and absent optionals`() {
        val row = """
            {
              "id": "wt-1",
              "device_row_id": "row-1",
              "repo_full_name": "acme/api",
              "branch": "exp/EXP-42",
              "issue_identifier": "EXP-42",
              "agents": ["claude"],
              "dirty": "clean",
              "busy": "t",
              "reported_at": "2026-08-11 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(DeviceWorktreeEntity.serializer(), row)
        assertEquals("exp/EXP-42", entity.branch)
        assertTrue(entity.busy)
        assertTrue(entity.agents!!.contains("claude"))

        val minimal = json.decodeFromString(
            DeviceWorktreeEntity.serializer(),
            """{"id": "wt-2", "device_row_id": "row-1"}""",
        )
        assertNull(minimal.issueIdentifier)
        assertNull(minimal.agents)
        assertFalse(minimal.busy)
        assertEquals("unknown", minimal.dirty)
    }
}
