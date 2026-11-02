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
