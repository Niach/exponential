import type { DeviceAgentAccounts } from "@/db/schema"
import type { AgentLoginProfileTarget } from "@/hooks/use-agent-login"

// EXP-827: "Add account" — the rules behind the sign-in a DEVICE row offers.
// It ends in an `agent_login` device command (`use-agent-login.ts`) that names
// WHERE the login lands on the machine: the ambient login when it is still
// free, or a NEW profile the machine creates first (`newProfileLabel`;
// EXP-792's per-agent config dirs). The machine's next probe reports the
// profile and the row grows a login by itself.
//
// EXP-909: every rule here is now per-DEVICE. The "which of my machines could
// take this login" search (`addAccountDevices`) and its disabled-control
// reasons (`addAccountBlockReason`) went with the cross-device Accounts
// section: the entry point is a row UNDER a machine, so the machine is the
// question, not the answer. The shapes below take either a synced `Device` row
// or a composed `SteerDevice`.

/** One machine's agent reporting, from either shape. */
type AgentReporting = {
  agents?: readonly string[] | null
  unauthedAgents?: readonly string[] | null
}

/** One machine's account reporting, from either shape. */
type AccountReporting = { agentAccounts?: DeviceAgentAccounts | null }

/** The server clamps a profile label at 64. */
export const MAX_PROFILE_LABEL = 64

/** The agent is INSTALLED on the machine — runnable (signed in) or signed
 * out; either can take a login. */
export function agentInstalledOn(
  row: AgentReporting,
  agent: string
): boolean {
  return (
    (row.agents ?? []).includes(agent) ||
    (row.unauthedAgents ?? []).includes(agent)
  )
}

/** The agents an "Add account" flow may sign in on the machine: every
 * installed one (EXP-849: all remaining agents have a device-code flow). */
export function addableAgents(row: AgentReporting): string[] {
  return [
    ...new Set([...(row.agents ?? []), ...(row.unauthedAgents ?? [])]),
  ]
}

/** Where a new login lands on the machine: the ambient login while it is
 * signed out (nothing to keep beside it), otherwise a new profile carrying
 * `label`. */
export function addAccountLoginTarget(
  row: AccountReporting,
  agent: string,
  label: string
): AgentLoginProfileTarget {
  const account = row.agentAccounts?.[agent]
  const ambient = account?.profiles?.find((profile) => profile.id === `system`)
  const ambientSignedIn = ambient ? ambient.signedIn : account?.signedIn === true
  if (!ambientSignedIn) return { profileId: `system` }
  return { newProfileLabel: clampProfileLabel(label) }
}

/** `Claude account 2` — the smallest N ≥ 2 whose `<agent> account N` is not
 * already the label of a profile the machine reports for the agent (exact,
 * case-sensitive). Counting profiles instead re-issued a label that still
 * existed after an earlier one was removed (`[system, "account 3"]` →
 * "account 3"), and `agentLoginLanded` then saw the sign-in as already
 * landed. iOS/Android `nextProfileLabel`, same rule. */
export function nextProfileLabel(
  row: AccountReporting,
  agent: string,
  agentLabel: string
): string {
  const taken = new Set(
    (row.agentAccounts?.[agent]?.profiles ?? []).map((profile) => profile.label ?? ``)
  )
  let n = 2
  while (taken.has(`${agentLabel} account ${n}`)) n += 1
  return clampProfileLabel(`${agentLabel} account ${n}`)
}

export function clampProfileLabel(label: string): string {
  const trimmed = label.trim()
  return trimmed.length > MAX_PROFILE_LABEL
    ? trimmed.slice(0, MAX_PROFILE_LABEL)
    : trimmed
}
