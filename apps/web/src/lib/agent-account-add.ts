import type { Device } from "@/db/schema"
import { deviceCanAgentLogin, deviceRowIsOnline } from "@/lib/steer-devices"
import type { AgentLoginProfileTarget } from "@/hooks/use-agent-login"

// EXP-827: "Add account" / "add this machine to an account" — the rules
// behind the Accounts section's two new entry points. Both end in an
// `agent_login` device command (`use-agent-login.ts`) that names WHERE the
// login lands on the machine: the ambient login when it is still free, or a
// NEW profile the machine creates first (`newProfileLabel`; EXP-792's
// per-agent config dirs). The machine's next probe reports the profile and
// the account card grows the chip by itself.

/** The server clamps a profile label at 64. */
export const MAX_PROFILE_LABEL = 64

/** The agent is INSTALLED on the machine — runnable (signed in) or signed
 * out; either can take a login. */
export function agentInstalledOn(
  row: Pick<Device, `agents` | `unauthedAgents`>,
  agent: string
): boolean {
  return (
    (row.agents ?? []).includes(agent) ||
    (row.unauthedAgents ?? []).includes(agent)
  )
}

/** The agents an "Add account" flow may sign in on the machine: every
 * installed one (EXP-849: all remaining agents have a device-code flow). */
export function addableAgents(
  row: Pick<Device, `agents` | `unauthedAgents`>
): string[] {
  return [
    ...new Set([...(row.agents ?? []), ...(row.unauthedAgents ?? [])]),
  ]
}

/** The caller's machines a sign-in can be queued on right now: own, online,
 * advertising `agent-login`, with `agent` installed when one is named, and
 * not among `exclude` (the machines already holding the account). */
export function addAccountDevices(
  rows: readonly Device[],
  opts: {
    currentUserId: string
    now: Date
    agent?: string
    exclude?: Iterable<string>
  }
): Device[] {
  const excluded = new Set(opts.exclude ?? [])
  return rows.filter(
    (row) =>
      row.userId === opts.currentUserId &&
      !excluded.has(row.deviceId) &&
      deviceRowIsOnline(row.lastSeenAt, opts.now) &&
      deviceCanAgentLogin({ caps: row.caps ?? [] }) &&
      (opts.agent ? agentInstalledOn(row, opts.agent) : addableAgents(row).length > 0)
  )
}

/** Where a new login lands on the machine: the ambient login while it is
 * signed out (nothing to keep beside it), otherwise a new profile carrying
 * `label`. */
export function addAccountLoginTarget(
  row: Pick<Device, `agentAccounts`>,
  agent: string,
  label: string
): AgentLoginProfileTarget {
  const account = row.agentAccounts?.[agent]
  const ambient = account?.profiles?.find((profile) => profile.id === `system`)
  const ambientSignedIn = ambient ? ambient.signedIn : account?.signedIn === true
  if (!ambientSignedIn) return { profileId: `system` }
  return { newProfileLabel: clampProfileLabel(label) }
}

/** `Claude account 2` — one past the profiles the machine reports for the
 * agent (the ambient login counts as the first). */
export function nextProfileLabel(
  row: Pick<Device, `agentAccounts`>,
  agent: string,
  agentLabel: string
): string {
  const profiles = row.agentAccounts?.[agent]?.profiles
  const count = profiles && profiles.length > 0 ? profiles.length : 1
  return clampProfileLabel(`${agentLabel} account ${count + 1}`)
}

export function clampProfileLabel(label: string): string {
  const trimmed = label.trim()
  return trimmed.length > MAX_PROFILE_LABEL
    ? trimmed.slice(0, MAX_PROFILE_LABEL)
    : trimmed
}
