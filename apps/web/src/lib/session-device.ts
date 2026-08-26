import type { CodingSession, Device } from "@/db/schema"
import type { SessionDisplayState } from "./coding-session-display"
import { deviceRowIsOnline } from "./steer-devices"

// EXP-549/550: how a coding session names its host machine, and whether that
// machine is currently offline. The session row carries `device_id` (the
// host's steer deviceId, stamped since EXP-549) plus a `device_label`
// SNAPSHOT; the synced `devices` row is the live source for both the RENAMED
// label and `last_seen_at` freshness. Hand-mirrored on iOS
// (SessionDevicePresentation.swift), Android (SessionDevicePresentation.kt)
// and desktop (queries.rs `session_device_presentation`).

export interface SessionDevice {
  /** The device's current label (registry row) or the row's snapshot; null
   * when the session never named a device. */
  label: string | null
  /** `false` = the resolved devices row is stale past the contract online
   * window; `null` = no devices row could be resolved (unknown, never
   * paused). */
  online: boolean | null
}

type SessionDeviceRow = Pick<
  CodingSession,
  `deviceId` | `deviceLabel` | `userId`
>
type DeviceLike = Pick<
  Device,
  `deviceId` | `label` | `userId` | `lastSeenAt`
>

/**
 * Resolve the session's device row by `device_id` (preferring the session
 * owner's own row). A row-less session — historical NULL-device_id rows —
 * falls back to the snapshot with unknown online-ness (EXP-560 retired the
 * unique-label guess: a mutable display name must not decide pause state).
 */
export function resolveSessionDevice(
  session: SessionDeviceRow,
  devices: readonly DeviceLike[],
  now: Date
): SessionDevice {
  let row: DeviceLike | undefined
  if (session.deviceId) {
    const matches = devices.filter((d) => d.deviceId === session.deviceId)
    row = matches.find((d) => d.userId === session.userId) ?? matches[0]
  }
  if (!row) return { label: session.deviceLabel, online: null }
  return {
    label: row.label || session.deviceLabel,
    online: deviceRowIsOnline(row.lastSeenAt, now),
  }
}

/** A running/needs-input session on an offline machine reads as PAUSED —
 * the agent is parked, not gone, and resumes when the device returns. Parked
 * review/done states stay what they are (the PR outcome is the story there,
 * not the machine). */
export function sessionIsPaused(
  state: SessionDisplayState,
  device: SessionDevice
): boolean {
  return (
    device.online === false && (state === `running` || state === `needs_input`)
  )
}
