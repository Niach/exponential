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
    fun `only the stamped device_id resolves a row — never the label`() {
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
        // EXP-560: no device_id resolves nothing at all, even when exactly one
        // row still carries the snapshot label — the guess is gone.
        val unstamped = resolveSessionDevice(session(deviceLabel = "macbook"), devices, nowMs)
        assertEquals("macbook", unstamped.label)
        assertFalse(unstamped.offline)
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

        val online = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook")),
            nowMs,
        )
        assertFalse(online.offline)
        assertFalse(online.isPaused(CodingSessionDisplayState.Running))
    }

    // ── EXP-656: presence is unknown while our own devices cursor is stale ───

    @Test
    fun `an unrefreshed devices cursor never pauses a session`() {
        // The exact report: back from the background, the devices shape has not
        // caught up, and the pre-sleep last_seen_at reads as away. It isn't —
        // we simply have not heard, so presence is unknown and nothing pauses.
        val resolved = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook", lastSeenAt = iso(-10 * 60_000))),
            nowMs,
            devicesFresh = false,
        )
        assertEquals(null, resolved.online)
        assertFalse(resolved.offline)
        assertFalse(resolved.isPaused(CodingSessionDisplayState.Running))
        assertFalse(resolved.isPaused(CodingSessionDisplayState.NeedsInput))
        // The label still resolves off the live row — only presence is unknown.
        assertEquals("macbook", resolved.label)
    }

    @Test
    fun `a fresh heartbeat is online even on a stale cursor`() {
        // last_seen_at only moves FORWARD, so a stale cursor can only produce a
        // false OFFLINE — an online verdict off one is always sound.
        val resolved = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook")),
            nowMs,
            devicesFresh = false,
        )
        assertEquals(true, resolved.online)
        assertFalse(resolved.offline)
    }

    @Test
    fun `a stale heartbeat on a FRESH cursor still pauses (EXP-550 guard)`() {
        val resolved = resolveSessionDevice(
            session(deviceId = "dev-1", deviceLabel = "macbook"),
            listOf(device("row-1", "dev-1", "macbook", lastSeenAt = iso(-10 * 60_000))),
            nowMs,
            devicesFresh = true,
        )
        assertEquals(false, resolved.online)
        assertTrue(resolved.offline)
        assertTrue(resolved.isPaused(CodingSessionDisplayState.Running))
    }

    @Test
    fun `no row at all is unknown, not online`() {
        assertEquals(null, resolveSessionDevice(session(), emptyList(), nowMs).online)
        assertEquals(null, SessionDevicePresentation.Unknown.online)
        assertFalse(SessionDevicePresentation.Unknown.offline)
    }
}
