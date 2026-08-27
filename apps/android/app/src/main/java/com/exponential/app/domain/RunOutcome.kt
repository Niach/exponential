package com.exponential.app.domain

import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.CodingSessionEntity

// EXP-637: how a FINISHED coding session presents itself, and when it can be
// resumed.
//
// An agent closes its own run out through the `exponential_sessions_end` MCP
// tool: a one-paragraph summary plus an outcome. Every client renders the same
// four labels for it (byte-equal across web, desktop, iOS and Android) — an
// ended row that carries no outcome (killed, merged, swept, or simply older
// than EXP-637) just reads "Ended".

/** The presentable outcome of an ended run — the glyph/tint selector too. */
enum class RunOutcome {
    Done,
    Blocked,
    NoChanges,

    /** Ended with no agent close-out: killed, merged, swept, or pre-EXP-637. */
    Ended,
    ;

    /** The one label every client shows for this outcome. */
    val label: String
        get() = when (this) {
            Done -> "Done"
            Blocked -> "Blocked"
            NoChanges -> "No changes"
            Ended -> "Ended"
        }
}

/**
 * The wire `outcome` value → its presentation. Anything unrecognized (a newer
 * server's value) falls back to [RunOutcome.Ended] rather than rendering a raw
 * wire token.
 */
fun runOutcomeOf(outcome: String?): RunOutcome = when (outcome) {
    DomainContract.codingSessionOutcomeDone -> RunOutcome.Done
    DomainContract.codingSessionOutcomeBlocked -> RunOutcome.Blocked
    DomainContract.codingSessionOutcomeNoChanges -> RunOutcome.NoChanges
    else -> RunOutcome.Ended
}

/** Shorthand for the label alone — the byte-equal string the row renders. */
fun runOutcomeLabel(outcome: String?): String = runOutcomeOf(outcome).label

/**
 * Where a Resume would go: the machine that ran the session, resolved against
 * the live device rows.
 */
data class RunResumeTarget(
    val sessionId: String,
    val deviceId: String,
    val deviceLabel: String,
)

/**
 * Whether [session] can be resumed right now, and on which machine (EXP-637).
 *
 * Everything the server checks, checked here first so the affordance is simply
 * absent instead of failing after the tap: the run must be the caller's OWN (a
 * teammate's run is never resumable — EXP-312), it must have ENDED (a live run
 * is steered, not resumed), it must carry the `device_id` of the machine that
 * ran it (that machine still holds the worktree and the agent's transcript),
 * and that machine must be online AND advertise the `resume-run` cap. The
 * machine may be a teammate's SHARED server (EXP-432) — the run is still the
 * caller's; own rows just win when the same machine id appears twice.
 *
 * Mirrors iOS `RunResume.target(for:devices:currentUserId:)`.
 */
fun resumeTargetFor(
    session: CodingSessionEntity,
    devices: List<SteerDevice>,
    currentUserId: String?,
): RunResumeTarget? {
    if (currentUserId == null || session.userId != currentUserId) return null
    if (session.status != DomainContract.codingSessionStatusEnded) return null
    val deviceId = session.deviceId?.takeIf { it.isNotEmpty() } ?: return null
    val candidates = devices.filter { it.deviceId == deviceId }
    val device = candidates.firstOrNull { it.isMine } ?: candidates.firstOrNull() ?: return null
    if (!device.online || !device.canResumeRun) return null
    return RunResumeTarget(
        sessionId = session.id,
        deviceId = device.deviceId,
        // Never blank: an unlabelled machine still reads as something.
        deviceLabel = device.deviceLabel.ifBlank { device.deviceId },
    )
}
