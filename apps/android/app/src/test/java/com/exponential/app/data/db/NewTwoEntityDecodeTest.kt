package com.exponential.app.data.db

import com.exponential.app.data.api.toPromptDto
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TriggerWhen
import com.exponential.app.domain.parseAgentTriggers
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * NEW-2: wire-decode vectors for the three new shapes, each row as the
 * server's pinned allowlist sends it (snake_case, jsonb as JSON): `agents`
 * (id, team_id, name, icon, color, triggers, device_id, cli, account, model,
 * owner_id, sort_order, created_at, updated_at), `events` (id, team_id, name,
 * payload, emitted_by, session_id, user_id, created_at) and `prompts` (id,
 * team_id, name, icon, sort_order, created_at, updated_at). A column absent
 * on the wire must never drop the row.
 */
class NewTwoEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `an agents row decodes with its triggers as raw jsonb`() {
        val row = """
            {
              "id": "ag-1",
              "team_id": "team-1",
              "name": "Triage",
              "icon": "bot",
              "color": "#22c55e",
              "triggers": [
                {"id": "t-1", "enabled": true, "kind": "schedule", "cron": "0 9 * * 1-5", "tz": "Europe/Vienna"},
                {"id": "t-2", "enabled": false, "kind": "event", "name": "pr.merged"}
              ],
              "device_id": "dev-1",
              "cli": "claude",
              "account": null,
              "model": "opus",
              "owner_id": "u-1",
              "sort_order": 2,
              "created_at": "2026-10-21 10:00:00+00",
              "updated_at": "2026-10-21 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(AgentDefinitionEntity.serializer(), row)
        assertEquals("Triage", entity.name)
        assertEquals("dev-1", entity.deviceId)
        assertNull(entity.account)
        assertEquals(2.0, entity.sortOrder, 0.0)
        val triggers = parseAgentTriggers(entity.triggers)
        assertEquals(TriggerWhen.Schedule("0 9 * * 1-5", "Europe/Vienna"), triggers[0].whenPart)
        assertEquals(TriggerWhen.Event("pr.merged"), triggers[1].whenPart)
        assertEquals(false, triggers[1].enabled)
        assertEquals(
            SessionTree.AgentDefinition("ag-1", "team-1", "Triage", "bot", "#22c55e", 2.0),
            SessionTree.agentDefinition(entity),
        )
        // A bare row (columns absent) still decodes.
        val bare = json.decodeFromString(AgentDefinitionEntity.serializer(), """{"id":"ag-2"}""")
        assertEquals("", bare.name)
        assertEquals(emptyList<Any>(), parseAgentTriggers(bare.triggers))
    }

    @Test
    fun `an events row decodes with its payload as raw jsonb`() {
        val row = """
            {
              "id": "ev-1",
              "team_id": "team-1",
              "name": "issue.created",
              "payload": {"issueId": "i-1", "identifier": "APP-1"},
              "emitted_by": "server",
              "session_id": null,
              "user_id": "u-1",
              "created_at": "2026-10-21 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(EventEntity.serializer(), row)
        assertEquals("issue.created", entity.name)
        assertEquals("server", entity.emittedBy)
        assertNull(entity.sessionId)
        assertEquals("u-1", entity.userId)
        assertEquals("""{"issueId":"i-1","identifier":"APP-1"}""", entity.payload)
    }

    @Test
    fun `a prompts row decodes body-less and maps to the UI row`() {
        val row = """
            {
              "id": "p-1",
              "team_id": "team-1",
              "name": "Review PR",
              "icon": null,
              "sort_order": 1.5,
              "created_at": "2026-10-21 10:00:00+00",
              "updated_at": "2026-10-21 11:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(PromptEntity.serializer(), row)
        val dto = entity.toPromptDto()
        assertEquals("Review PR", dto.name)
        assertNull(dto.icon)
        assertEquals("", dto.body)
        assertEquals(1.5, dto.sortOrder, 0.0)
        assertEquals("team-1", dto.teamId)
    }
}
