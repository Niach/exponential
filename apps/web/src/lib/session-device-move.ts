// EXP-1252: moving a live SESSION to another device, from its composer's
// options line (the launch line's Device pin, placed before Model).
//
// A move = ONE `sessions.send({ sessionId, deviceId })` with no text
// (NEW-1): a queued row re-targets; a live one ENDS here and continues on
// the target in a new session on the same branch (logins stay).
//
// The rows are THE launch composer's machines for this team (own + shared
// into it, `useSteerDevices`), each with its refusal. Every sentence is
// hand-mirrored ×4 (desktop `crates/ui/src/device_move.rs`, Android
// `domain/SessionDeviceMove.kt`, iOS `Domain/SessionDeviceMove.swift`);
// `session-device-move.test.ts` locks them against the native sources.
import { isPersistent } from "@exp/domain-contract/session-kind"
import { flattenAccounts } from "@/lib/accounts/account-option"
import { parseVersionTuple } from "@/lib/client-version"
import type { SteerDevice } from "@/lib/steer-devices"

export { canSteerSession } from "@/lib/session-account-switch"

/** The picker's title (the phone sheet, the pin's accessible name). */
export const PICKER_TITLE = `Move to device`

/** The pin while a move is pending; `{device}` = the target's name. */
export const MOVING_LABEL = `Moving to {device}…`

/** What a move does, said once where the move is offered. */
export const NOTE = `The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device.`

/** A person's failed move (the failedAction toast title). */
export const MOVE_FAILED_TOAST = `Couldn't move the session`

// ── Refusals: one sentence each, shown ON the disabled control ─────────────

export const REASON_STEER = `Only people who can steer this session can move it.`
export const REASON_ENDED = `This session has ended.`
export const REASON_AGENT = `Agents move from Settings → Agents.`
export const REASON_ASIDE = `An aside stays on its parent's device.`
export const REASON_OFFLINE = `This device is offline.`
export const REASON_VISIBILITY = `This device is shared differently, so the session can't move here.`
export const REASON_NO_LOGIN = `This device has no signed-in login for this agent.`
export const REASON_UPDATE = `This device needs an update to receive sessions.`

/** The first desktop/CLI build that receives a handoff (the server's
 *  `MOVE_MIN_VERSION`); an unreported version = an old build. */
export const MOVE_MIN_VERSION: readonly [number, number, number] = [0, 14, 73]

export function deviceCanReceiveSession(version: string | null | undefined): boolean {
  if (!version) return false
  const have = parseVersionTuple(version)
  if (!have) return false
  for (let i = 0; i < 3; i++) {
    if (have[i]! !== MOVE_MIN_VERSION[i]!) return have[i]! > MOVE_MIN_VERSION[i]!
  }
  return true
}

export function movingLabel(deviceName: string): string {
  return MOVING_LABEL.replace(`{device}`, deviceName)
}

/** The pin renders at all: never for an agent's conversation (NEW-2: a
 *  session with `agentId`, persistent; it moves with its agent's device in
 *  Settings → Agents) nor an aside (it stays on its parent's device). */
export function deviceMoveVisible(session: {
  agentId?: string | null
  aside?: boolean | null
}): boolean {
  return deviceMoveHiddenReason(session) === null
}

/** Why the pin is hidden (the server's refusal for the same request). */
export function deviceMoveHiddenReason(session: {
  agentId?: string | null
  aside?: boolean | null
}): string | null {
  if (isPersistent(session)) return REASON_AGENT
  if (session.aside) return REASON_ASIDE
  return null
}

/** The whole pin's refusal, or null. ORDER ×4: steer, then ended. */
export function deviceMoveBlockedReason(input: {
  canSteer: boolean
  ended: boolean
}): string | null {
  if (!input.canSteer) return REASON_STEER
  if (input.ended) return REASON_ENDED
  return null
}

/** The device row is shared into the session's team. */
export function deviceSharedWithTeam(
  device: Pick<SteerDevice, `sharedTeamIds`>,
  teamId: string
): boolean {
  return (device.sharedTeamIds ?? []).includes(teamId)
}

/** The agent a session runs (the server's rule): its `cli`, else its
 *  current device's default agent, else claude. */
export function deviceMoveAgent(
  cli: string | null | undefined,
  currentDevice: Pick<SteerDevice, `launchDefaults`> | null | undefined
): string {
  return cli ?? currentDevice?.launchDefaults?.defaultAgent ?? `claude`
}

/** One target's refusal, or null = movable. ORDER ×4: offline, visibility,
 *  update (a build before `MOVE_MIN_VERSION`), no login (no signed-in login
 *  of the session's agent there). */
export function deviceMoveTargetReason(
  device: Pick<SteerDevice, `online` | `sharedTeamIds` | `agentAccounts` | `version`>,
  session: { teamId: string; shared: boolean; agent: string }
): string | null {
  if (device.online === false) return REASON_OFFLINE
  if (deviceSharedWithTeam(device, session.teamId) !== session.shared)
    return REASON_VISIBILITY
  if (!deviceCanReceiveSession(device.version)) return REASON_UPDATE
  const signedIn = flattenAccounts(device).some((option) => option.agent === session.agent)
  if (!signedIn) return REASON_NO_LOGIN
  return null
}

export interface SessionDeviceMoveRow {
  id: string
  name: string
  icon?: string | null
  kind?: string | null
  /** The session is on it now (selected; picking it cancels a pending move). */
  current: boolean
  /** The pending move's target. */
  target: boolean
  reason: string | null
}

/** The launch composer's machine name: label + EXP-432's owner suffix. */
export function deviceMoveName(
  device: Pick<SteerDevice, `deviceId` | `deviceLabel` | `owner`>
): string {
  return `${device.deviceLabel || device.deviceId}${
    device.owner ? ` — ${device.owner.name}` : ``
  }`
}

/** The rows, in the device list's order; the current device always listed
 *  (never refused), every other one with its refusal. */
export function sessionDeviceMoveRows(
  devices: readonly SteerDevice[],
  session: {
    deviceId: string | null
    moveToDeviceId?: string | null
    teamId: string
    shared: boolean
    cli: string | null | undefined
  }
): SessionDeviceMoveRow[] {
  const agent = deviceMoveAgent(
    session.cli,
    devices.find((device) => device.deviceId === session.deviceId)
  )
  const rows = devices.map((device) => {
    const current = device.deviceId === session.deviceId
    return {
      id: device.deviceId,
      name: deviceMoveName(device),
      icon: device.icon ?? null,
      kind: device.kind ?? null,
      current,
      target: Boolean(session.moveToDeviceId) && device.deviceId === session.moveToDeviceId,
      reason: current
        ? null
        : deviceMoveTargetReason(device, { ...session, agent }),
    }
  })
  if (session.deviceId && !rows.some((row) => row.current)) {
    rows.unshift({
      id: session.deviceId,
      name: session.deviceId,
      icon: null,
      kind: null,
      current: true,
      target: false,
      reason: null,
    })
  }
  return rows
}

/** What a pick does: `move` (a new target), `cancel` (the current device
 *  while moving) or null (nothing to do). */
export function deviceMovePick(
  picked: string,
  session: { deviceId: string | null; moveToDeviceId?: string | null }
): `move` | `cancel` | null {
  if (picked === session.deviceId) return session.moveToDeviceId ? `cancel` : null
  if (picked === session.moveToDeviceId) return null
  return `move`
}
