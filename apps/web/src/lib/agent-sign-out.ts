import { agentLabel } from "@exp/ui"
import type { DeviceAgentAccounts, DeviceAgentHealth } from "@exp/db-schema"

// #EXP-1534: a device's login going healthy → signed_out / needs_relogin
// notifies its owner ONCE. Pure: `devices.register` / `devices.heartbeat`
// hand it the stored report, the incoming one and the device's
// `agent_sign_out_notified` keys, and write back `notified`.
//
// A login = one profile row of an agent, or the agent row itself when it
// reports no profiles. It FIRES when it reads signed_out / needs_relogin now,
// the previous report held it healthy (signed in, health ok or unknown) and
// its key is not in `notified`. Its key leaves `notified` only when it reads
// `ok` again. So a restart (a register re-reporting the dead login, or
// `unknown` before its first probe) and every later heartbeat stay quiet, and
// an agent installed but never signed in never fires.

export interface AgentSignOut {
  key: string
  agent: string
  profileId: string | null
  email: string | null
  health: Extract<DeviceAgentHealth, `signed_out` | `needs_relogin`>
}

interface Login {
  key: string
  agent: string
  profileId: string | null
  email: string | null
  signedIn: boolean
  health: DeviceAgentHealth | undefined
}

export function agentSignOutKey(agent: string, profileId: string | null): string {
  return profileId ? `${agent}/${profileId}` : agent
}

function logins(accounts: DeviceAgentAccounts | null | undefined): Map<string, Login> {
  const out = new Map<string, Login>()
  for (const [agent, account] of Object.entries(accounts ?? {})) {
    if (account.profiles && account.profiles.length > 0) {
      for (const profile of account.profiles) {
        const key = agentSignOutKey(agent, profile.id)
        out.set(key, {
          key,
          agent,
          profileId: profile.id,
          email: profile.email ?? null,
          signedIn: profile.signedIn === true,
          health: profile.health,
        })
      }
    } else {
      out.set(agent, {
        key: agent,
        agent,
        profileId: null,
        email: account.email ?? null,
        signedIn: account.signedIn === true,
        health: account.health,
      })
    }
  }
  return out
}

const isDead = (health: DeviceAgentHealth | undefined) =>
  health === `signed_out` || health === `needs_relogin`

export function detectAgentSignOuts(
  previous: DeviceAgentAccounts | null | undefined,
  next: DeviceAgentAccounts,
  notified: readonly string[]
): { signOuts: AgentSignOut[]; notified: string[] } {
  const before = logins(previous)
  const keys = new Set(notified)
  const signOuts: AgentSignOut[] = []
  for (const login of logins(next).values()) {
    if (login.health === `ok`) {
      keys.delete(login.key)
      continue
    }
    if (!isDead(login.health) || keys.has(login.key)) continue
    const was = before.get(login.key)
    if (!was || !was.signedIn || isDead(was.health)) continue
    keys.add(login.key)
    signOuts.push({
      key: login.key,
      agent: login.agent,
      profileId: login.profileId,
      // A signed-out profile may no longer name its login: the last report did.
      email: login.email ?? was.email,
      health: login.health as AgentSignOut[`health`],
    })
  }
  return { signOuts, notified: [...keys].sort() }
}

/** The row's title (and the push's): "Claude Code account a@b.dev is signed
 *  out on mint". */
export function agentSignOutTitle(signOut: AgentSignOut, deviceLabel: string): string {
  const account = signOut.email ? ` account ${signOut.email}` : ``
  return `${agentLabel(signOut.agent)}${account} is signed out on ${deviceLabel}`
}
