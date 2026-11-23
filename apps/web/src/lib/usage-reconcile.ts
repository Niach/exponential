// #FEED-304: what the SERVER knows about a login's usage that one machine
// cannot. A device reports the numbers it last read; it cannot say that they
// have aged out while it was offline, and it cannot know that the same account
// was read a minute ago on the machine next to it. A MacBook showed an account
// at Week 100 % from a 21-hour-old read, `stale: false`, while the same
// account read 62 % on another machine, and sessions avoided it.
//
// Two rules, pure functions of the owner's device rows and a clock:
//
//  - STALE: numbers older than `USAGE_STALE_MS`, or carrying a window whose
//    `resetsAt` has passed, are `stale: true` whatever the device said.
//  - NEWEST WINS: one account (same agent, same email, case-insensitive) has
//    ONE set of limits, so every device's row for it takes the newest read
//    any of the owner's devices reported.
//
// Applied on write (`settleSharedUsage`, lib/trpc/devices.ts: every register
// and heartbeat that carries a report) so all four clients read agreeing rows
// off sync, and on read by the MCP devices list (an offline machine's row
// ages with nobody writing it).

import type {
  DeviceAgentAccounts,
  DeviceAgentProfileEntry,
  DeviceAgentUsage,
  DeviceAgentUsageMap,
} from "@/db/schema"

/** Numbers older than this are not current: twice the slowest schedule a
 * device polls on (`usage_cache::MAX_PIN_SECS`), the device's own
 * `WIRE_STALE_AFTER_SECS`. */
export const USAGE_STALE_MS = 60 * 60 * 1000

/** A `fetchedAt` this far past the server's clock is a machine whose clock
 * runs ahead: it never outranks another device's read. */
export const USAGE_CLOCK_SKEW_MS = 5 * 60 * 1000

function stampMs(value: string | null | undefined): number | null {
  if (!value) return null
  const ms = new Date(value).getTime()
  return Number.isNaN(ms) ? null : ms
}

/** Whether `usage` no longer describes the account: the device said so, it
 * is older than `USAGE_STALE_MS`, a window it carries has reset, or it names
 * no readable `fetchedAt` at all. */
export function usageIsStale(
  usage: DeviceAgentUsage | null | undefined,
  now: Date
): boolean {
  if (!usage) return false
  if (usage.stale === true) return true
  const fetched = stampMs(usage.fetchedAt)
  if (fetched === null) return true
  if (now.getTime() - fetched >= USAGE_STALE_MS) return true
  return (usage.windows ?? []).some((window) => {
    const reset = stampMs(window.resetsAt)
    return reset !== null && reset <= now.getTime()
  })
}

/** One device's two usage columns. */
export interface DeviceUsageState {
  agentAccounts: DeviceAgentAccounts | null
  agentUsage: DeviceAgentUsageMap | null
}

export interface ReconciledUsage<T> extends DeviceUsageState {
  row: T
  /** Whether either column differs from what `row` held. */
  changed: boolean
}

function accountKey(agent: string, profile: DeviceAgentProfileEntry): string | null {
  const email = profile.email?.trim().toLowerCase()
  if (!email || profile.signedIn !== true) return null
  return `${agent}\n${email}`
}

function sameUsage(a: DeviceAgentUsage | undefined, b: DeviceAgentUsage | undefined): boolean {
  if (!a || !b) return a === b
  if (a.fetchedAt !== b.fetchedAt || (a.stale === true) !== (b.stale === true)) return false
  const left = a.windows ?? []
  const right = b.windows ?? []
  return (
    left.length === right.length &&
    left.every((window, index) => {
      const other = right[index]!
      return (
        window.key === other.key &&
        window.label === other.label &&
        window.percent === other.percent &&
        (window.resetsAt ?? null) === (other.resetsAt ?? null)
      )
    })
  )
}

/** `usage` with the server's stale verdict; the same object when it holds. */
function withStale(usage: DeviceAgentUsage, now: Date): DeviceAgentUsage {
  const stale = usageIsStale(usage, now)
  return (usage.stale === true) === stale ? usage : { ...usage, stale }
}

/** Both rules over ONE owner's device rows (never across owners: a
 * teammate's login is theirs). Rows come back in order, each with its two
 * columns as they should read at `now`. */
export function reconcileDeviceUsage<T extends DeviceUsageState>(
  rows: readonly T[],
  now: Date
): ReconciledUsage<T>[] {
  // The newest read of each account any of the rows holds.
  const newest = new Map<string, { usage: DeviceAgentUsage; at: number; plan?: string }>()
  for (const row of rows) {
    for (const [agent, account] of Object.entries(row.agentAccounts ?? {})) {
      for (const profile of account?.profiles ?? []) {
        const key = accountKey(agent, profile)
        const at = stampMs(profile.usage?.fetchedAt)
        if (!key || !profile.usage || at === null) continue
        if ((profile.usage.windows ?? []).length === 0) continue
        if (at > now.getTime() + USAGE_CLOCK_SKEW_MS) continue
        const held = newest.get(key)
        if (!held || at > held.at) {
          newest.set(key, { usage: profile.usage, at, plan: profile.plan })
        }
      }
    }
  }

  return rows.map((row) => {
    let changed = false
    let accounts = row.agentAccounts
    let usageMap = row.agentUsage
    for (const [agent, account] of Object.entries(row.agentAccounts ?? {})) {
      if (!account?.profiles) continue
      let profilesChanged = false
      const profiles = account.profiles.map((profile) => {
        const key = accountKey(agent, profile)
        const best = key ? newest.get(key) : undefined
        const own = stampMs(profile.usage?.fetchedAt)
        // A login the device deliberately collects nothing for keeps saying
        // so; a plan that disagrees is another subscription of that address.
        const adopt =
          best !== undefined &&
          profile.unmonitored !== true &&
          (!best.plan || !profile.plan || best.plan === profile.plan) &&
          (own === null || best.at > own)
        const base = adopt ? best.usage : profile.usage
        if (!base) return profile
        const next = withStale(base, now)
        if (sameUsage(next, profile.usage)) return profile
        profilesChanged = true
        const updated: DeviceAgentProfileEntry = { ...profile, usage: next }
        // The numbers are another machine's read now, not this login's
        // token probe.
        if (adopt) delete updated.usageSource
        return updated
      })
      if (profilesChanged) {
        changed = true
        accounts = { ...accounts, [agent]: { ...account, profiles } }
      }
      // The top-level map is the ACTIVE login's numbers.
      const active = profiles.find((profile) => profile.active === true)
      if (active?.usage && usageMap?.[agent] && !sameUsage(active.usage, usageMap[agent])) {
        changed = true
        usageMap = { ...usageMap, [agent]: active.usage }
      }
    }
    // A device that reports no profiles (an older build) still ages.
    for (const [agent, usage] of Object.entries(usageMap ?? {})) {
      if (!usage || accounts?.[agent]?.profiles?.some((profile) => profile.active === true && profile.usage)) {
        continue
      }
      const next = withStale(usage, now)
      if (next !== usage) {
        changed = true
        usageMap = { ...usageMap, [agent]: next }
      }
    }
    return { row, agentAccounts: accounts, agentUsage: usageMap, changed }
  })
}
