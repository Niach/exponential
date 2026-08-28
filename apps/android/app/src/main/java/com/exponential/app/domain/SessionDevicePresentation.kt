package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity

// EXP-549/550: how a coding session presents its HOST MACHINE.
//
// `coding_sessions.device_label` is only the start-time snapshot — renaming
// the machine afterwards left every running row labelled with the original
// hostname (EXP-549). The LIVE devices row wins whenever we can find it, and
// its `last_seen_at` freshness is also what tells us the machine went away
// (lid closed): the session is then PAUSED, not lost and not ended — it
// resumes when the machine comes back (EXP-550), so nothing here ends or
// re-dials anything, it only changes what the surfaces render.

/**
 * The resolved machine identity for one session row: the label to render and
 * what we know about that machine's presence.
 *
 * [online] is deliberately nullable (EXP-656, web `resolveSessionDevice`
 * parity): null = we do not know — no devices row, or a devices cursor we have
 * not refreshed inside the contract window, which can only ever produce a
 * FALSE offline (see [DeviceFreshness]). Unknown never pauses anything.
 */
data class SessionDevicePresentation(
    val label: String?,
    val online: Boolean?,
) {
    /** Known to be away. Unknown presence is NOT offline. */
    val offline: Boolean get() = online == false

    /** Never blank — an unlabelled/unknown machine still reads as something. */
    val displayLabel: String get() = label?.takeIf { it.isNotBlank() } ?: "Desktop"

    /**
     * Whether the session should read "paused" instead of live. Only a session
     * that would otherwise render as still-working can pause — a row already
     * in review / done is parked on its own outcome, and an offline
     * machine says nothing about it.
     */
    fun isPaused(state: CodingSessionDisplayState): Boolean =
        offline && (
            state == CodingSessionDisplayState.Running ||
                state == CodingSessionDisplayState.NeedsInput
            )

    companion object {
        val Unknown = SessionDevicePresentation(label = null, online = null)
    }
}

/**
 * Join [session] to its live [devices] row and derive the presentation.
 *
 * The ONLY match is the row whose steer `device_id` equals the session's
 * stamped one — preferring the session owner's own row, since a shared server
 * machine (EXP-432) can appear once per user. The legacy `device_label`
 * unique-match fallback is gone (EXP-560): those pre-stamp rows have drained,
 * and guessing a machine by name could land on a DIFFERENT one.
 *
 * With no row we cannot know anything about presence, so the snapshot label
 * renders and presence stays UNKNOWN — an unknown machine (or a null
 * `deviceId`) must never fake a paused session.
 *
 * [devicesFresh] (EXP-656) says whether our own `devices` shape has polled
 * recently enough for a stale `last_seen_at` to mean anything. It defaults to
 * true so a call site that has no freshness signal behaves exactly as before;
 * every real site passes it.
 */
fun resolveSessionDevice(
    session: CodingSessionEntity,
    devices: List<DeviceEntity>,
    nowMs: Long,
    devicesFresh: Boolean = true,
): SessionDevicePresentation {
    val deviceId = session.deviceId
    val row = deviceId?.let {
        val matches = devices.filter { device -> device.deviceId == it }
        matches.firstOrNull { device -> device.userId == session.userId } ?: matches.firstOrNull()
    }
    if (row == null) {
        return SessionDevicePresentation(label = session.deviceLabel, online = null)
    }
    val live = DeviceLiveness.isOnline(row.lastSeenAt, nowMs)
    return SessionDevicePresentation(
        label = row.label.takeIf { it.isNotBlank() } ?: session.deviceLabel,
        // A fresh heartbeat is online whatever our cursor did; only the
        // NEGATIVE needs a cursor we trust.
        online = if (live) true else if (devicesFresh) false else null,
    )
}
