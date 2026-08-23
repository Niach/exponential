package com.exponential.app.domain

import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.DeviceWorktreeEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-481: the synced-row → SteerDevice mapping + the resume matcher. */
class DeviceRowsTest {

    private val nowMs = 1_754_900_000_000L
    private fun iso(deltaMs: Long): String =
        java.time.Instant.ofEpochMilli(nowMs + deltaMs).toString()

    private fun entity(over: DeviceEntity.() -> DeviceEntity = { this }): DeviceEntity =
        DeviceEntity(
            id = "row-1",
            userId = "me",
            deviceId = "dev-1",
            label = "buildbox",
            kind = "server",
            agents = """["claude","codex"]""",
            caps = """["actions","resume","worktrees","launch-defaults"]""",
            unauthedAgents = """["pi"]""",
            launchDefaults = """{"defaultAgent":"codex","agents":{"codex":{"model":""}}}""",
            activeSessions = 1,
            lastSeenAt = iso(-10_000),
            updateRequestedAt = iso(-5_000),
        ).over()

    // ── DeviceLiveness ───────────────────────────────────────────────────────

    @Test
    fun `online inside the 90s window, offline at and past it`() {
        assertTrue(DeviceLiveness.isOnline(iso(-89_000), nowMs))
        assertFalse(DeviceLiveness.isOnline(iso(-90_000), nowMs))
        assertFalse(DeviceLiveness.isOnline(iso(-90_001), nowMs))
    }

    @Test
    fun `negative age (server clock ahead) clamps online`() {
        assertTrue(DeviceLiveness.isOnline(iso(30_000), nowMs))
    }

    @Test
    fun `unparseable or absent last_seen_at fails CLOSED — offline`() {
        assertFalse(DeviceLiveness.isOnline("not a timestamp", nowMs))
        assertFalse(DeviceLiveness.isOnline(null, nowMs))
    }

    // ── toSteerDevice ────────────────────────────────────────────────────────

    @Test
    fun `maps the synced row — jsonb decoded, online stamped, updateBlocked derived`() {
        val device = entity().toSteerDevice(nowMs, currentUserId = "me")
        assertEquals("dev-1", device.deviceId)
        assertEquals("row-1", device.rowId)
        assertTrue(device.online)
        assertTrue(device.registered)
        assertEquals(listOf("claude", "codex"), device.agents)
        assertEquals(listOf("pi"), device.unauthedAgents)
        assertTrue(device.canResume)
        assertEquals("codex", device.launchDefaults?.defaultAgent)
        // updateRequested + activeSessions > 0 = blocked ("Update queued").
        assertTrue(device.updateRequested)
        assertTrue(device.updateBlocked)
        // Own row: no owner, isMine holds.
        assertNull(device.owner)
        assertTrue(device.isMine)
    }

    @Test
    fun `a teammate's row carries its owner`(): Unit {
        val device = entity { copy(userId = "them") }
            .toSteerDevice(nowMs, currentUserId = "me", ownerName = "Danny")
        assertEquals("them", device.owner?.id)
        assertEquals("Danny", device.owner?.name)
        assertFalse(device.isMine)
    }

    // EXP-622: the flag is the ROW OWNER's preference — reading a teammate's
    // shared server must never prefill the caller's picker with it.
    @Test
    fun `isDefault survives on an own row and is dropped on a teammate's`() {
        assertTrue(
            entity { copy(isDefault = true) }
                .toSteerDevice(nowMs, currentUserId = "me").isDefault,
        )
        assertFalse(
            entity { copy(userId = "them", isDefault = true) }
                .toSteerDevice(nowMs, currentUserId = "me", ownerName = "Danny").isDefault,
        )
        assertFalse(entity().toSteerDevice(nowMs, currentUserId = "me").isDefault)
    }

    @Test
    fun `malformed jsonb degrades field-wise, never drops the row`() {
        val device = entity {
            copy(agents = "not json", launchDefaults = "{broken")
        }.toSteerDevice(nowMs, currentUserId = "me")
        // Absent advertisement semantics: agents null = claude-only fallback.
        assertNull(device.agents)
        assertNull(device.launchDefaults)
    }

    // ── resumeWorktreeFor ────────────────────────────────────────────────────

    private fun worktree(over: DeviceWorktreeEntity.() -> DeviceWorktreeEntity = { this }) =
        DeviceWorktreeEntity(
            id = "wt-1",
            deviceRowId = "row-1",
            repoFullName = "acme/api",
            branch = "exp/EXP-42",
            issueIdentifier = "EXP-42",
            agents = """["claude"]""",
        ).over()

    @Test
    fun `matches identifier case-insensitively on the same device row`() {
        val match = resumeWorktreeFor(listOf(worktree()), "row-1", "exp-42", "claude")
        assertEquals("wt-1", match?.id)
    }

    @Test
    fun `agent must be in the marker — null marker means any agent`() {
        assertNull(resumeWorktreeFor(listOf(worktree()), "row-1", "EXP-42", "codex"))
        val anyAgent = worktree { copy(agents = null) }
        assertEquals(
            "wt-1",
            resumeWorktreeFor(listOf(anyAgent), "row-1", "EXP-42", "codex")?.id,
        )
    }

    @Test
    fun `wrong device row or missing identifier never matches`() {
        assertNull(resumeWorktreeFor(listOf(worktree()), "row-2", "EXP-42", "claude"))
        assertNull(resumeWorktreeFor(listOf(worktree()), null, "EXP-42", "claude"))
        assertNull(resumeWorktreeFor(listOf(worktree()), "row-1", null, "claude"))
        val foreign = worktree { copy(issueIdentifier = null) }
        assertNull(resumeWorktreeFor(listOf(foreign), "row-1", "EXP-42", "claude"))
    }
}
