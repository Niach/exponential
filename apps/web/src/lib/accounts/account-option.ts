// EXP-988 contract: the ONE account option model every composer offers
// (owner: EXP-872, which contains EXP-977; the Rust, Swift and Kotlin mirrors
// are declared by that node itself — nobody else consumes them).
//
// One flattened list REPLACES the agent picker + the account picker on every
// platform. The rules, which `flattenAccounts` implements and
// account-option.test.ts locks:
//
//  - one option per signed-in login the device reports, across both contract
//    agents (`devices.agent_accounts[agent].profiles`; a device that reports
//    no profiles yields its ambient `system` login);
//  - the label is ALWAYS the agent's brand mark + the email — never the
//    profile name (`label`), never the word "default". A login the device
//    reports without an address shows its plan; with neither, its profile id
//    (the machine has nothing better);
//  - the DEVICE DEFAULT is marked by ORDER (it is first) and by a check, not
//    by a label: `isDeviceDefault` is true for exactly one option — the
//    `launch_defaults.defaultAccount` profile of `defaultAgent` when the
//    device stores one, else the active login of `defaultAgent`, falling back
//    to the first contract agent's active login. The rest follow in
//    `sortDeviceLogins` order (lib/agent-usage.ts);
//  - selecting an option IMPLIES the agent: there is no separate agent pick,
//    `agent` rides the option and the launch takes both from it;
//  - "default agent" settings become "default account": the setting stores a
//    profile id, and the agent is derived from it.
//
// `limits` are FRACTIONS 0..1 off the usage windows EXP-909 settled
// (`DeviceUsageWindow.percent / 100`): `fiveHour` = the `session` window,
// `week` = the `weekly` window, `model` = the FIRST per-model window
// (`label` = the model's name, `used` = its fraction). A login with no usage
// report has no `limits` at all.
//
// Hand-mirrored ×4 against the same fixture and the same test names:
//   desktop  apps/desktop/crates/coding/src/account_option.rs
//   iOS      apps/ios/ExpCore/Sources/Domain/AccountOption.swift
//   Android  apps/android/.../domain/AccountOption.kt
import type {
  DeviceAgentAccounts,
  DeviceAgentHealth,
  DeviceAgentUsageMap,
  DeviceLaunchDefaults,
} from "@/db/schema"
import { contract } from "@exp/domain-contract"
import {
  deviceLoginRows,
  sortDeviceLogins,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"

export interface AccountOption {
  /** The profile id (`agent_profiles`), what a launch passes as `account`. */
  id: string
  agent: `claude` | `codex`
  /** What the row SAYS (beside the brand mark). See the header for the
   * fallbacks when the device reported no address. */
  email: string
  /** Exactly one option per device is the default; it is also listed first. */
  isDeviceDefault: boolean
  /** EXP-849: the device's verdict on the credential — a run started on a
   * dead login dies on its first call, so the row badges `needs_relogin`. */
  health: DeviceAgentHealth
  limits?: {
    fiveHour: number
    week: number
    model?: { label: string; used: number }
  }
}

/** What ONE device reports, as either the synced row or the composed
 * `SteerDevice`. */
export interface AccountSource {
  agentAccounts?: DeviceAgentAccounts | null
  agentUsage?: DeviceAgentUsageMap | null
  launchDefaults?: DeviceLaunchDefaults | null
}

/** `${agent}:${profileId}` — the ONE string a `<select>`-shaped picker can
 * carry for an option, since a profile id alone (`system`) repeats across
 * agents. `parseAccountOptionKey` reads it back. */
export function accountOptionKey(
  option: Pick<AccountOption, `agent` | `id`>
): string {
  return `${option.agent}:${option.id}`
}

export function parseAccountOptionKey(
  key: string
): { agent: string; id: string } | null {
  const at = key.indexOf(`:`)
  if (at <= 0 || at === key.length - 1) return null
  return { agent: key.slice(0, at), id: key.slice(at + 1) }
}

/** The email a row reads as — see the header's fallback ladder. */
function optionEmail(row: AgentProfileUsageRow): string {
  return row.email || row.plan || row.profileId
}

function fraction(percent: number | undefined): number {
  if (percent === undefined || !Number.isFinite(percent)) return 0
  return Math.min(1, Math.max(0, percent / 100))
}

function optionLimits(
  row: AgentProfileUsageRow
): AccountOption[`limits`] | undefined {
  const windows = row.usage?.windows ?? []
  if (windows.length === 0) return undefined
  const session = windows.find((window) => window.key === `session`)
  const weekly = windows.find((window) => window.key === `weekly`)
  const model = windows.find((window) => window.key.startsWith(`model:`))
  return {
    fiveHour: fraction(session?.percent),
    week: fraction(weekly?.percent),
    ...(model ? { model: { label: model.label, used: fraction(model.percent) } } : {}),
  }
}

export function flattenAccounts(device: AccountSource): AccountOption[] {
  // `deviceLoginRows` already knows the shape: one row per profile (or the
  // ambient `system` login for a profile-less agent), retired agents dropped,
  // the active profile's numbers read off either slot.
  const rows = sortDeviceLogins(
    deviceLoginRows(
      {
        deviceId: ``,
        deviceLabel: ``,
        agentAccounts: device.agentAccounts ?? undefined,
        agentUsage: device.agentUsage ?? undefined,
      },
      { mine: true, online: true }
    )
  ).filter((row) => row.signedIn)
  if (rows.length === 0) return []

  // The default: the stored default account of the configured default agent,
  // else that agent's active login, else the first contract agent's active
  // login, else the first row — never none.
  const order = contract.codingAgent.values as readonly string[]
  const configured = device.launchDefaults?.defaultAgent
  const configuredAccount = device.launchDefaults?.defaultAccount
  const activeOf = (agent: string | undefined) =>
    agent ? rows.find((row) => row.agent === agent && row.active) : undefined
  const stored =
    configured && configuredAccount
      ? rows.find(
          (row) => row.agent === configured && row.profileId === configuredAccount
        )
      : undefined
  const defaultRow =
    stored ??
    activeOf(configured) ??
    order.map((agent) => activeOf(agent)).find(Boolean) ??
    rows[0]!

  const ordered = [defaultRow, ...rows.filter((row) => row !== defaultRow)]
  return ordered.map((row) => {
    const limits = optionLimits(row)
    return {
      id: row.profileId,
      agent: row.agent as AccountOption[`agent`],
      email: optionEmail(row),
      isDeviceDefault: row === defaultRow,
      health: row.health,
      ...(limits ? { limits } : {}),
    }
  })
}

/** The option a launch surface should START on: the device default, or the
 * first option. `undefined` for a device that reports no login at all. */
export function defaultAccountOption(
  options: readonly AccountOption[]
): AccountOption | undefined {
  return options.find((option) => option.isDeviceDefault) ?? options[0]
}
