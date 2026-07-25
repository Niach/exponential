import { contract } from "@exp/domain-contract"

// The caller's online desktops as `steer.myDevices` returns them — extracted
// from the Start-coding dialog (EXP-257) so the launch dialog, hooks, and the
// Agents page share one shape + capability vocabulary.

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
  /** EXP-201: agent CLIs installed on the device; absent = claude-only. */
  agents?: string[]
  /** EXP-253: launch capabilities beyond issue coding (e.g. `actions`);
   * absent = an older desktop with none. */
  caps?: string[]
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
