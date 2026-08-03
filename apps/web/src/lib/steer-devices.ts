import { contract } from "@exp/domain-contract"

// The caller's machines as `devices.list` returns them (EXP-403: the durable
// registry merged with live relay presence) — the launch dialog, hooks, and
// the Agents page share one shape + capability vocabulary. Rows straight off
// the relay (`steer.myDevices`) still fit: the registry fields are optional
// and absent-`online` reads as online.

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  /** Relay presence timestamp — absent on registry-only (offline) rows. */
  connectedAt?: number
  /** EXP-201: agent CLIs installed on the device; absent = claude-only. */
  agents?: string[]
  /** EXP-253: launch capabilities beyond issue coding (e.g. `actions`);
   * absent = an older desktop with none. */
  caps?: string[]
  /** EXP-403 registry fields (devices.list). */
  kind?: `desktop` | `server`
  online?: boolean
  /** ISO timestamp of the last register/heartbeat; null for relay-only rows. */
  lastSeenAt?: string | null
  registered?: boolean
  /** Marketing version as of the last register; null for old builds. */
  version?: string | null
  /** A web "Update" click is pending on the device. */
  updateRequested?: boolean
}

/** Whether the device is startable right now. Rows without the field come
 * straight off the relay presence map and are online by construction. */
export function deviceIsOnline(device: SteerDevice): boolean {
  return device.online !== false
}

/** Agents the device can run — absent advertisement means claude-only. */
export function deviceAgentIds(device: SteerDevice | undefined): string[] {
  return device?.agents && device.agents.length > 0
    ? device.agents.filter((a) => contract.codingAgent.values.includes(a))
    : [`claude`]
}

/** Only desktops advertising this capability have an action launch path —
 * older builds can run claude but not actions (`steer.startSession` enforces
 * the same server-side). */
export function deviceCanRunActions(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`actions`)
}

/** Builtin or inputs-carrying action runs additionally need this capability
 * (EXP-257) — an older desktop would silently drop the inputs field and run
 * a valueless prompt (`steer.startSession` enforces the same server-side). */
export function deviceCanRunActionInputs(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`action-inputs`)
}

/** The builtin "Fix merge conflicts" run needs this capability (EXP-259) —
 * `steer.startSession` rejects the builtin without it, so filter such desktops
 * out of the picker instead of failing after submit (EXP-323). */
export function deviceCanFixConflicts(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`fix-conflicts`)
}
