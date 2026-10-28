import type { Device, Session } from "@/db/schema"
import { deviceRowIsOnline } from "./steer-devices"

// EXP-549/550/1291: how a session names its host machine, and whether that
// machine is currently offline. The session row carries `device_id` (the
// host's steer deviceId); the synced `devices` row is the live source for
// the label (its RENAME) and `last_seen_at` freshness. Hand-mirrored on iOS
// (SessionDevicePresentation.swift), Android (SessionDevicePresentation.kt)
// and desktop (queries.rs `session_device_presentation`).

export interface SessionDevice {
  /** The device's current label; null when no devices row resolves. */
  label: string | null
  /** `false` = the resolved devices row is stale past the contract online
   * window; `null` = no devices row could be resolved (unknown, never
   * paused). */
  online: boolean | null
  /** The resolved row's synced caps; absent = no row, or none reported. */
  caps?: string[]
}

type SessionDeviceRow = Pick<Session, `deviceId` | `userId`>
type DeviceLike = Pick<Device, `deviceId` | `label` | `userId` | `lastSeenAt`> & {
  caps?: unknown
}

/** Resolve the session's device row by `device_id`, preferring the session
 *  starter's own row (two users see one machine through a shared row). */
export function resolveSessionDevice(
  session: SessionDeviceRow,
  devices: readonly DeviceLike[],
  now: Date
): SessionDevice {
  const matches = devices.filter((d) => d.deviceId === session.deviceId)
  const row = matches.find((d) => d.userId === session.userId) ?? matches[0]
  if (!row) return { label: null, online: null }
  return {
    label: row.label || null,
    online: deviceRowIsOnline(row.lastSeenAt, now),
    ...(Array.isArray(row.caps)
      ? { caps: row.caps.filter((cap): cap is string => typeof cap === `string`) }
      : {}),
  }
}
