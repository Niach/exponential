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
//    by a label: `isDeviceDefault` is true for exactly one option — the active
//    login of `launch_defaults.defaultAgent`, falling back to the first
//    contract agent's active login. The rest follow in `sortDeviceLogins`
//    order (lib/agent-usage.ts);
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
import type {
  DeviceAgentAccounts,
  DeviceAgentUsageMap,
  DeviceLaunchDefaults,
} from "@/db/schema"

export interface AccountOption {
  /** The profile id (`agent_profiles`), what a launch passes as `account`. */
  id: string
  agent: `claude` | `codex`
  /** What the row SAYS (beside the brand mark). See the header for the
   * fallbacks when the device reported no address. */
  email: string
  /** Exactly one option per device is the default; it is also listed first. */
  isDeviceDefault: boolean
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

export function flattenAccounts(_device: AccountSource): AccountOption[] {
  throw new Error(`flattenAccounts is not implemented yet (EXP-872 owns it)`)
}
