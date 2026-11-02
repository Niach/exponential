// EXP-827: "Add account" — the rules behind the sign-in a DEVICE row offers.
// It ends in an `agent_login` device command (`use-agent-login.ts`) with NO
// profile: the machine signs in inside a fresh staging dir and lands the login
// by its EMAIL — an address it already holds refreshes that profile, a new one
// becomes a new profile (EXP-792's per-agent config dirs). The machine's next
// heartbeat reports it and the row grows a login by itself.
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

/** EXP-1626: the longest name a long-lived Add may carry (the server's
 *  `AGENT_PROFILE_NAME_MAX`, the device's cap). */
export const ACCOUNT_NAME_MAX = 64

/** EXP-1626: the `name` an `agent_login` carries — a setup token cannot name
 *  its account, so only a long-lived ADD (no `profileId`) names the profile
 *  it makes; trimmed, blank = none (the device's `keptSignedInName`). */
export function agentLoginName(target: {
  profileId?: string
  longLived?: boolean
  name?: string
}): string | undefined {
  if (!target.longLived || target.profileId) return undefined
  const name = target.name?.trim().slice(0, ACCOUNT_NAME_MAX)
  return name ? name : undefined
}

/** EXP-1626: the sign-in Add account hands the dialog: claude carries the
 *  Name field's trimmed text (blank sends none); other agents never do. */
export function addAccountTarget(
  agent: string,
  name: string
): { agent: string; name?: string } {
  const trimmed = agent === `claude` ? name.trim() : ``
  return trimmed ? { agent, name: trimmed } : { agent }
}
