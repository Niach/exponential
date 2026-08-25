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
 * whether that machine is currently offline.
 */
data class SessionDevicePresentation(
    val label: String?,
    val offline: Boolean,
) {
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
        val Unknown = SessionDevicePresentation(label = null, offline = false)
    }
}

/**
 * Join [session] to its live [devices] row and derive the presentation.
 *
 * Match order:
 *  1. The row whose steer `device_id` equals the session's stamped one —
 *     preferring the session owner's own row, since a shared server machine
 *     (EXP-432) can appear once per user.
 *  2. ONLY for rows started before the stamp existed (null `deviceId`): the
 *     UNIQUE row whose label still equals the snapshot. Ambiguous (2+) or
 *     absent means no row — a renamed machine simply keeps its snapshot
 *     rather than risking the wrong machine's presence.
 *
 * With no row we cannot know anything about presence, so the snapshot label
 * renders and `offline` stays false — an unknown machine must never fake a
 * paused session.
 */
fun resolveSessionDevice(
    session: CodingSessionEntity,
    devices: List<DeviceEntity>,
    nowMs: Long,
): SessionDevicePresentation {
    val deviceId = session.deviceId
    val row = if (deviceId != null) {
        // A stamped-but-unknown machine stays unresolved — never fall through
        // to the label guess, which could land on a DIFFERENT machine.
        val matches = devices.filter { it.deviceId == deviceId }
        matches.firstOrNull { it.userId == session.userId } ?: matches.firstOrNull()
    } else {
        val snapshot = session.deviceLabel?.takeIf { it.isNotBlank() }
        snapshot?.let { label -> devices.filter { it.label == label }.singleOrNull() }
    }
    if (row == null) {
        return SessionDevicePresentation(label = session.deviceLabel, offline = false)
    }
    return SessionDevicePresentation(
        label = row.label.takeIf { it.isNotBlank() } ?: session.deviceLabel,
        offline = !DeviceLiveness.isOnline(row.lastSeenAt, nowMs),
    )
}
