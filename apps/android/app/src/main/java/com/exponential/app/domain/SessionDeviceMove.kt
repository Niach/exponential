package com.exponential.app.domain

import com.exponential.app.data.api.SteerDevice

// EXP-1252: moving a live SESSION to another device, from its composer's
// options line (the launch line's Device pin, placed before Model).
//
// A move is ONE `sessions.send({ sessionId, deviceId })` with no text
// ([com.exponential.app.data.api.SessionsSendInput.moveToDevice]) (NEW-1): a
// queued row re-targets; a live one ENDS here and continues on the target in
// a new session on the same branch (logins stay). NEW-8: nothing is pending
// on the row, so there is no target to show and nothing to cancel.
//
// Everything here is pure. Hand-mirrored ×4 (web `lib/session-device-move.ts`,
// desktop `crates/ui/src/device_move.rs`, iOS `SessionDeviceMove.swift`): same
// refusals, same order, same strings — web's `session-device-move.test.ts`
// reads this file.

/** One machine row of the move picker. */
data class SessionDeviceMoveRow(
    val id: String,
    val name: String,
    /** Contract `deviceIcon`; null = the kind default. */
    val icon: String?,
    val isServer: Boolean,
    /** The session is on it now (selected; picking it does nothing). */
    val current: Boolean,
    /** Why it cannot take the session, or null = movable. */
    val reason: String?,
)

/** What a pick does. */
enum class DeviceMovePick { Move }

object SessionDeviceMove {

    /** The picker's title (the phone sheet, the pin's accessible name). */
    const val PICKER_TITLE = "Move to device"

    /** What a move does, said once where the move is offered. */
    const val NOTE = "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."

    /** A person's failed move (the failure toast). */
    const val MOVE_FAILED_TOAST = "Couldn't move the session"

    // ── Refusals: one sentence each, shown ON the disabled control ──────────

    const val REASON_STEER = "Only people who can steer this session can move it."
    const val REASON_ENDED = "This session has ended."
    const val REASON_AGENT = "Agents move from Settings → Agents."
    const val REASON_OFFLINE = "This device is offline."
    const val REASON_VISIBILITY = "This device is shared differently, so the session can't move here."
    const val REASON_NO_LOGIN = "This device has no signed-in login for this agent."

    /** The agent a session without a recorded `cli` runs (web parity). */
    private const val DEFAULT_AGENT = "claude"

    /** Why the whole pin is HIDDEN (the server's refusal for the same
     *  request), or null = it renders: never for an agent's conversation
     *  (persistent; it moves from Settings → Agents). */
    fun hiddenReason(kind: String?): String? =
        if (SessionKind.sessionTraits(kind).persistent) REASON_AGENT else null

    fun visible(kind: String?): Boolean = hiddenReason(kind) == null

    /** A shared session is steerable by every member, any other only by its
     *  starter (Threads v2; web `canSteerSession`). */
    fun canSteer(shared: Boolean, starterId: String, currentUserId: String?): Boolean =
        shared || (!currentUserId.isNullOrEmpty() && starterId == currentUserId)

    /** The whole pin's refusal, or null. ORDER ×4: steer, then ended. */
    fun blockedReason(canSteer: Boolean, ended: Boolean): String? = when {
        !canSteer -> REASON_STEER
        ended -> REASON_ENDED
        else -> null
    }

    /** The device row is shared into the session's team. */
    fun sharedWithTeam(device: SteerDevice, teamId: String): Boolean = teamId in device.sharedTeamIds

    /** #EXP-1440: the refusal for an own machine shared only with OTHER
     *  teams (the server's FORBIDDEN, the picker's hidden row). */
    const val DEVICE_OTHER_TEAM_MESSAGE = "This device belongs to another team"

    /** #EXP-1440: one of the caller's OWN machines may run a team's sessions
     *  when it is personal (shared with no team) or shared with that team; a
     *  machine shared only with other teams is theirs (web `deviceUsableInTeam`). */
    fun deviceUsableInTeam(device: SteerDevice, teamId: String): Boolean =
        device.sharedTeamIds.isEmpty() || sharedWithTeam(device, teamId)

    /** One target's refusal, or null = movable. ORDER ×4: offline,
     *  visibility, no login. Old builds are the CLIENT_MIN_VERSION floor's
     *  job, never a refusal here. */
    fun targetReason(device: SteerDevice, teamId: String, shared: Boolean, cli: String?): String? {
        if (!device.online) return REASON_OFFLINE
        if (sharedWithTeam(device, teamId) != shared) return REASON_VISIBILITY
        val agent = cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT
        if (agent !in device.runnableAgents) return REASON_NO_LOGIN
        return null
    }

    /** The launch composer's machine name: label + EXP-432's owner suffix. */
    fun name(device: SteerDevice): String {
        val base = device.deviceLabel.ifBlank { device.deviceId }
        return device.owner?.let { "$base — ${it.name}" } ?: base
    }

    /** The rows, in the device list's order; the current device always
     *  listed (never refused), every other one with its refusal. */
    fun rows(
        devices: List<SteerDevice>,
        deviceId: String?,
        teamId: String,
        shared: Boolean,
        cli: String?,
    ): List<SessionDeviceMoveRow> {
        val rows = devices.map { device ->
            val current = device.deviceId == deviceId
            SessionDeviceMoveRow(
                id = device.deviceId,
                name = name(device),
                icon = device.icon,
                isServer = device.isServer,
                current = current,
                reason = if (current) null else targetReason(device, teamId, shared, cli),
            )
        }
        if (!deviceId.isNullOrEmpty() && rows.none { it.current }) {
            return listOf(
                SessionDeviceMoveRow(
                    id = deviceId,
                    name = deviceId,
                    icon = null,
                    isServer = false,
                    current = true,
                    reason = null,
                ),
            ) + rows
        }
        return rows
    }

    /** The pin's label: the current device's name. */
    fun pinLabel(rows: List<SessionDeviceMoveRow>, deviceId: String?): String =
        rows.firstOrNull { it.current }?.name ?: deviceId.orEmpty()

    /** What a pick does: [DeviceMovePick.Move] (another device) or null (the
     *  current one). */
    fun pick(picked: String, deviceId: String?): DeviceMovePick? =
        if (picked == deviceId) null else DeviceMovePick.Move
}
