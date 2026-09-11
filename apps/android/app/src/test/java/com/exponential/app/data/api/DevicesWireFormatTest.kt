package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-481: wire-format locks for the device-settings mutations. FEED-33:
 * `devices.setShared` is the per-team toggle form (`shared` true/false), no
 * nullable `teamId` any more.
 */
class DevicesWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `setShared emits the per-team toggle form`() {
        assertEquals(
            """{"deviceId":"dev-1","teamId":"team-1","shared":true}""",
            json.encodeToString(
                SetSharedInput.serializer(),
                SetSharedInput(deviceId = "dev-1", teamId = "team-1", shared = true),
            ),
        )
        assertEquals(
            """{"deviceId":"dev-1","teamId":"team-1","shared":false}""",
            json.encodeToString(
                SetSharedInput.serializer(),
                SetSharedInput(deviceId = "dev-1", teamId = "team-1", shared = false),
            ),
        )
    }

    @Test
    fun `command builders emit the flat server payloads`() {
        assertEquals(
            """{"deviceId":"dev-1","kind":"worktree_remove","repoFullName":"acme/api","branch":"exp/EXP-42"}""",
            worktreeRemoveCommand("dev-1", "acme/api", "exp/EXP-42").toString(),
        )
        assertEquals(
            """{"deviceId":"dev-1","kind":"worktree_prune"}""",
            worktreePruneCommand("dev-1").toString(),
        )
        assertEquals(
            """{"deviceId":"dev-1","kind":"agent_login","agent":"claude","switch":false}""",
            agentLoginCommand("dev-1", "claude", false).toString(),
        )
        // EXP-765: the return path for claude's browser code — the server
        // trims it, so the client sends what the user typed.
        assertEquals(
            """{"deviceId":"dev-1","kind":"agent_login_code","agent":"claude","code":"ABC-123"}""",
            agentLoginCodeCommand("dev-1", "claude", "ABC-123").toString(),
        )
    }

    @Test
    fun `startSession input carries resume only when set`() {
        val without = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(issueId = "issue-1", deviceId = "dev-1"),
        )
        assertFalse(without.contains("resume"))

        val with = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(issueId = "issue-1", deviceId = "dev-1", resume = true),
        )
        assertTrue(with.contains(""""resume":true"""))
    }

    @Test
    fun `command dto decodes terminal states`() {
        val done = json.decodeFromString(
            DeviceCommandDto.serializer(),
            """{"id":"cmd-1","kind":"worktree_prune","status":"done","result":"Pruned 2 worktrees"}""",
        )
        assertTrue(done.isTerminal)
        assertEquals("Pruned 2 worktrees", done.result)

        val pending = json.decodeFromString(
            DeviceCommandDto.serializer(),
            """{"id":"cmd-1","kind":"worktree_prune","status":"pending","result":null,"completedAt":null,"createdAt":"2026-08-11T10:00:00Z","payload":{}}""",
        )
        assertFalse(pending.isTerminal)
    }
}
