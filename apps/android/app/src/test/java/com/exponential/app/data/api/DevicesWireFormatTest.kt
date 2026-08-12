package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-481: wire-format locks for the device-settings mutations. The
 * high-stakes one is `devices.setShared` with a LITERAL `"teamId":null` —
 * the server input is required-and-nullable, and the shared Json's
 * explicitNulls=false would silently drop a synthesized null, turning "stop
 * sharing" into BAD_REQUEST.
 */
class DevicesWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `setShared clear emits a literal teamId null`() {
        assertEquals(
            """{"deviceId":"dev-1","teamId":null}""",
            setSharedInput("dev-1", null).toString(),
        )
        assertEquals(
            """{"deviceId":"dev-1","teamId":"team-1"}""",
            setSharedInput("dev-1", "team-1").toString(),
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
