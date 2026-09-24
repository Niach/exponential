import { agentLabel } from "@exp/ui"

import { flattenAccounts, type AccountOption, type AccountSource } from "@/lib/accounts/account-option"
import { SYSTEM_PROFILE_ID } from "@/lib/agent-usage"

/**
 * EXP-1020: the "Default account" row's options in the DEVICE SETTINGS — the
 * machine's reported logins, plus one AMBIENT option per editable agent that
 * reports none.
 *
 * Not the same question a launch surface asks. Starting a run needs an agent
 * that can actually run there, so the composer keeps the strict list; this
 * row asks "which agent is this machine's default", and a machine with one
 * claude login must still be pointable at codex. The all-or-nothing fallback
 * it replaced only kicked in when the machine reported NO login at all, so
 * such a machine offered exactly one row — which the picker renders as a
 * plain label, i.e. the default could not be changed at all.
 *
 * Siblings: iOS `DeviceSettingsSheet.accountOptions`, Android
 * `deviceAccountOptions`.
 */
export function deviceAccountOptions(
  device: AccountSource | null,
  editorAgents: readonly string[]
): AccountOption[] {
  const reported = device ? flattenAccounts(device) : []
  const covered = new Set(reported.map((option) => option.agent))
  const ambient: AccountOption[] = editorAgents
    .filter((agent) => !covered.has(agent as AccountOption[`agent`]))
    .map((agent) => ({
      id: SYSTEM_PROFILE_ID,
      agent: agent as AccountOption[`agent`],
      email: agentLabel(agent),
      // The reported logins carry the device default among them; an agent
      // that reports nothing never is one.
      isDeviceDefault: false,
      health: `unknown` as const,
    }))
  return [...reported, ...ambient]
}
