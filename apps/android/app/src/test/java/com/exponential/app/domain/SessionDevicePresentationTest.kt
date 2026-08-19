package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-549/550: the session → live devices row join (label + paused). */
class SessionDevicePresentationTest {

    private val nowMs = 1_754_900_000_000L
    private fun iso(deltaMs: Long): String =
        java.time.Instant.ofEpochMilli(nowMs + deltaMs).toString()

    private fun session(
        deviceId: String? = null,
        deviceLabel: String? = null,
        userId: String = "me",
    ) = CodingSessionEntity(
        id = "sess-1",
        issueId = "issue-1",
        teamId = "team-1",
        userId = userId,
        deviceLabel = deviceLabel,
        deviceId = deviceId,
        startedAt = "2026-08-19T09:00:00Z",
        createdAt = "2026-08-19T09:00:00Z",
        updatedAt = "2026-08-19T09:00:00Z",
    )

    private fun device(
        id: String,
        deviceId: String,
        label: String,
        userId: String = "me",
        lastSeenAt: String? = null,
    ) = DeviceEntity(
        id = id,
        userId = userId,
        deviceId = deviceId,
        label = label,
        lastSeenAt = lastSeenAt ?: iso(-10_000),
    )

    @Test
    fun `the live row's label beats the start-time snapshot`() {
        val resolved = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "MacBook-Pro-von-Danny.local"),
            listOf(device("row-1", "dev-1", "macbook")),
            nowMs,
        )
        assertEquals("macbook", resolved.label)
        assertEquals("macbook", resolved.displayLabel)
        assertFalse(resolved.offline)
    }

    @Test
    fun `device_id wins over a same-label row, and the owner's row wins a shared id`() {
        val devices = listOf(
            device("row-other", "dev-1", "buildbox", userId = "someone-else"),
            device("row-mine", "dev-1", "buildbox", userId = "me"),
            device("row-label", "dev-2", "snapshot-name"),
        )
        val resolved = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "snapshot-name"),
            devices,
            nowMs,
        )
        assertEquals("buildbox", resolved.label)
    }

    @Test
    fun `the label fallback only applies to rows with no stamped device_id`() {
        val devices = listOf(device("row-1", "dev-1", "macbook", lastSeenAt = iso(-10 * 60_000)))
        // Stamped but unknown machine: no row, so the snapshot renders and
        // presence stays unknown (never a fake pause).
        val stamped = resolveSessionDevice(
            session(deviceId = "dev-gone", deviceLabel = "macbook"),
            devices,
            nowMs,
        )
        assertEquals("macbook", stamped.label)
        assertFalse(stamped.offline)
        // Legacy row (no device_id): the unique same-label row matches, and its
        // stale heartbeat reads offline.
        val legacy = resolveSessionDevice(session(deviceLabel = "macbook"), devices, nowMs)
        assertEquals("macbook", legacy.label)
        assertTrue(legacy.offline)
    }

    @Test
    fun `an ambiguous label keeps the snapshot and claims nothing about presence`() {
        val devices = listOf(
            device("row-1", "dev-1", "macbook", lastSeenAt = iso(-10 * 60_000)),
            device("row-2", "dev-2", "macbook", userId = "someone-else", lastSeenAt = iso(-10 * 60_000)),
        )
        val resolved = resolveSessionDevice(session(deviceLabel = "macbook"), devices, nowMs)
        assertEquals("macbook", resolved.label)
        assertFalse(resolved.offline)
    }

    @Test
    fun `no row at all falls back to the snapshot, blank label reads Desktop`() {
        val resolved = resolveSessionDevice(session(deviceLabel = null), emptyList(), nowMs)
        assertEquals(null, resolved.label)
        assertEquals("Desktop", resolved.displayLabel)
        assertFalse(resolved.offline)
    }

    @Test
    fun `a stale heartbeat pauses only the still-working states`() {
        val offline = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook", lastSeenAt = iso(-10 * 60_000))),
            nowMs,
        )
        assertTrue(offline.offline)
        assertTrue(offline.isPaused(CodingSessionDisplayState.Running))
        assertTrue(offline.isPaused(CodingSessionDisplayState.NeedsInput))
        assertFalse(offline.isPaused(CodingSessionDisplayState.Review))
        assertFalse(offline.isPaused(CodingSessionDisplayState.Done))
        assertFalse(offline.isPaused(CodingSessionDisplayState.Merged))

        val online = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook")),
            nowMs,
        )
        assertFalse(online.offline)
        assertFalse(online.isPaused(CodingSessionDisplayState.Running))
    }
}
