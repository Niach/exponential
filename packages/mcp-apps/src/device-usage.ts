import type { MeterTone } from "@exp/ui"
import { ago, resetCountdown } from "./list-session"

// EXP-1183 — the Devices view's pure half: the `exponential_devices_list`
// wire row (tools.ts) and the presentation rules the app's Devices page
// applies to it, ported from `apps/web/src/lib/agent-usage.ts` (EXP-849/909,
// hand-mirrored ×4: login rows, their order, health badges, the mini usage
// windows, severity) and `components/device-setup.tsx` (the status line).
// Parsing stays tolerant: a newer device's payload must never brick a view.

export type DeviceHealth = `ok` | `needs_relogin` | `signed_out` | `unknown`

export interface DeviceUsageWindow {
  key: string
  label: string
  percent: number
  resetsAt: string | null
}

export interface DeviceUsage {
  fetchedAt?: string
  stale?: boolean
  windows: DeviceUsageWindow[]
}

/** One `exponential_devices_list` row (tools.ts). */
export interface DeviceListRow {
  deviceId: string
  label?: string | null
  kind?: string | null
  platform?: string | null
  online?: boolean
  lastSeenAt?: string | null
  agents?: string[] | null
  unauthedAgents?: string[] | null
  caps?: string[] | null
  version?: string | null
  sharedTeamIds?: string[] | null
  isDefault?: boolean
  agentAccounts?: Record<string, unknown> | null
  agentUsage?: Record<string, unknown> | null
  agentUsageAt?: string | null
  owner?: { id: string; name: string }
}

/** One login a machine holds (`AgentProfileUsageRow`, trimmed). */
export interface DeviceLogin {
  key: string
  agent: string
  profileId: string
  profileLabel: string
  active: boolean
  signedIn: boolean
  health: DeviceHealth
  email: string | null
  plan: string | null
  usage: DeviceUsage | null
  unmonitored: boolean
  checkedAt: string | null
}

/** The shipped agents in contract order (`contract.codingAgent`). */
export const DEVICE_AGENT_ORDER = [`claude`, `codex`] as const

export const DEVICE_AGENT_LABEL: Record<string, string> = {
  claude: `Claude Code`,
  codex: `Codex`,
}

export const SYSTEM_PROFILE_ID = `system`
export const NO_LOGIN_REPORTED = `No login reported`
export const NO_EMAIL_LABEL = `No email`
const USAGE_FRESH_MS = 15 * 60 * 1000
const MAX_USAGE_WINDOWS = 10

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

const str = (value: unknown): string | null =>
  typeof value === `string` && value.length > 0 ? value : null

function parseWindow(value: unknown): DeviceUsageWindow | null {
  if (!isRecord(value)) return null
  const key = str(value.key)
  const label = str(value.label)
  if (!key || !label) return null
  const raw = typeof value.percent === `number` ? value.percent : 0
  const percent = Number.isFinite(raw) ? Math.min(100, Math.max(0, Math.round(raw))) : 0
  return { key, label, percent, resetsAt: str(value.resetsAt) }
}

export function parseDeviceUsage(value: unknown): DeviceUsage | null {
  if (!isRecord(value)) return null
  const windows: DeviceUsageWindow[] = []
  if (Array.isArray(value.windows)) {
    for (const entry of value.windows) {
      if (windows.length >= MAX_USAGE_WINDOWS) break
      const window = parseWindow(entry)
      if (window) windows.push(window)
    }
  }
  const usage: DeviceUsage = { windows }
  if (typeof value.fetchedAt === `string`) usage.fetchedAt = value.fetchedAt
  if (typeof value.stale === `boolean`) usage.stale = value.stale
  return usage
}

const HEALTHS: readonly DeviceHealth[] = [`ok`, `needs_relogin`, `signed_out`, `unknown`]

/** The device's verdict, else derived from `signedIn` (pre-EXP-849). */
export function loginHealth(entry: unknown): DeviceHealth {
  if (!isRecord(entry)) return `unknown`
  if (HEALTHS.includes(entry.health as DeviceHealth)) return entry.health as DeviceHealth
  return entry.signedIn === true ? `ok` : `signed_out`
}

/** `Needs re-login` / `Signed out`, or null when there is nothing to say. */
export function healthBadgeLabel(health: DeviceHealth): string | null {
  if (health === `needs_relogin`) return `Needs re-login`
  if (health === `signed_out`) return `Signed out`
  return null
}

function healthRank(health: DeviceHealth): number {
  if (health === `needs_relogin`) return 0
  if (health === `signed_out`) return 1
  if (health === `unknown`) return 2
  return 3
}

const isShippedAgent = (agent: string): boolean =>
  (DEVICE_AGENT_ORDER as readonly string[]).includes(agent)

/** EXP-909: every login ONE machine reports, agent by agent, sorted the ×4
 *  way: contract agent order, the ACTIVE login first, then a dead credential,
 *  then label and id so a heartbeat never reshuffles two equal rows. */
export function deviceLogins(device: DeviceListRow): DeviceLogin[] {
  const accounts = isRecord(device.agentAccounts) ? device.agentAccounts : {}
  const usageMap: Record<string, DeviceUsage> = {}
  if (isRecord(device.agentUsage)) {
    for (const [agent, entry] of Object.entries(device.agentUsage)) {
      const usage = parseDeviceUsage(entry)
      if (usage) usageMap[agent] = usage
    }
  }
  const agents = new Set(
    [...Object.keys(accounts), ...Object.keys(usageMap)].filter(isShippedAgent)
  )
  const out: DeviceLogin[] = []
  for (const agent of agents) {
    const account = isRecord(accounts[agent]) ? (accounts[agent] as Record<string, unknown>) : null
    const profiles = Array.isArray(account?.profiles)
      ? (account.profiles as unknown[]).filter(isRecord)
      : []
    if (profiles.length === 0) {
      out.push({
        key: `${agent}:${SYSTEM_PROFILE_ID}`,
        agent,
        profileId: SYSTEM_PROFILE_ID,
        profileLabel: `Default`,
        active: true,
        signedIn: account?.signedIn === true,
        health: loginHealth(account),
        email: str(account?.email),
        plan: str(account?.plan),
        usage: usageMap[agent] ?? null,
        unmonitored: false,
        checkedAt: str(account?.checkedAt) ?? device.agentUsageAt ?? null,
      })
      continue
    }
    for (const profile of profiles) {
      const id = str(profile.id) ?? SYSTEM_PROFILE_ID
      const active = profile.active === true
      out.push({
        key: `${agent}:${id}`,
        agent,
        profileId: id,
        profileLabel: str(profile.label) ?? (id === SYSTEM_PROFILE_ID ? `Default` : id),
        active,
        signedIn: profile.signedIn === true,
        health: loginHealth(profile),
        email: str(profile.email),
        plan: str(profile.plan),
        // The active login's numbers may ride only the pre-profile slot.
        usage: parseDeviceUsage(profile.usage) ?? (active ? (usageMap[agent] ?? null) : null),
        unmonitored: profile.unmonitored === true,
        checkedAt: str(profile.checkedAt) ?? str(account?.checkedAt),
      })
    }
  }
  const agentRank = (agent: string) =>
    (DEVICE_AGENT_ORDER as readonly string[]).indexOf(agent)
  return out.sort((a, b) => {
    const byAgent = agentRank(a.agent) - agentRank(b.agent)
    if (byAgent !== 0) return byAgent
    if (a.active !== b.active) return a.active ? -1 : 1
    const byAttention = attentionRank(a) - attentionRank(b)
    if (byAttention !== 0) return byAttention
    return (
      a.profileLabel.localeCompare(b.profileLabel) || a.profileId.localeCompare(b.profileId)
    )
  })
}

function attentionRank(login: DeviceLogin): number {
  if (!login.signedIn) return 0
  const peak = Math.max(0, ...(login.usage?.windows ?? []).map((w) => w.percent))
  return peak >= DANGER_PERCENT ? 1 : 2
}

/** The worst health across a machine's logins — what its row badges. */
export function deviceWorstHealth(logins: readonly DeviceLogin[]): DeviceHealth | null {
  let worst: DeviceHealth | null = null
  for (const login of logins) {
    if (worst === null || healthRank(login.health) < healthRank(worst)) worst = login.health
  }
  return worst
}

/** EXP-1013: a login's ONE name — its email, else its plan, else `No email`. */
export function loginName(login: Pick<DeviceLogin, `email` | `plan`>): string {
  return login.email?.trim() || login.plan?.trim() || NO_EMAIL_LABEL
}

/** EXP-862: what a login row says about its numbers. */
export type UsageState = `ready` | `checking` | `unmonitored` | `none`

export function usageState(login: DeviceLogin): UsageState {
  if (!login.signedIn) return `none`
  if (login.unmonitored) return `unmonitored`
  return (login.usage?.windows.length ?? 0) > 0 ? `ready` : `checking`
}

/** EXP-909: the short line — the 5h window, the week, the first per-model
 *  one (wire labels), else the first three. */
export function miniWindows(usage: DeviceUsage | null | undefined): DeviceUsageWindow[] {
  const windows = usage?.windows ?? []
  const picked: DeviceUsageWindow[] = []
  const session = windows.find((window) => window.key === `session`)
  if (session) picked.push(session)
  const weekly = windows.find((window) => window.key === `weekly`)
  if (weekly) picked.push(weekly)
  const model = windows.find((window) => window.key.startsWith(`model:`))
  if (model) picked.push(model)
  return picked.length > 0 ? picked : windows.slice(0, 3)
}

/** EXP-944: only the 5h and weekly windows caption their reset. */
export function windowReset(window: DeviceUsageWindow, now = Date.now()): string | null {
  if (window.key !== `session` && window.key !== `weekly`) return null
  return resetCountdown(window.resetsAt, now)
}

export const WARNING_PERCENT = 75
export const DANGER_PERCENT = 95

export function severity(percent: number): MeterTone {
  if (percent >= DANGER_PERCENT) return `danger`
  if (percent >= WARNING_PERCENT) return `warning`
  return `normal`
}

/** EXP-909: `as of 18m ago` once the numbers are past 15 minutes or the
 *  device marked them stale; null while current. */
export function usageAge(usage: DeviceUsage | null | undefined, now = Date.now()): string | null {
  if (!usage) return null
  const fetched = usage.fetchedAt ? new Date(usage.fetchedAt).getTime() : Number.NaN
  const fresh = !Number.isNaN(fetched) && now - fetched < USAGE_FRESH_MS
  if (fresh && usage.stale !== true) return null
  const when = ago(usage.fetchedAt, now)
  return when ? `as of ${when}` : null
}

/** `device-setup.tsx` `DeviceStatusLine`: Online, else when it was last seen. */
export function deviceStatusText(device: DeviceListRow, now = Date.now()): string {
  if (device.online) return `Online`
  const when = ago(device.lastSeenAt, now)
  return when ? `Last seen ${when}` : `Offline`
}

/** The platform as people say it (`macos` → `macOS`). */
export function platformLabel(platform: string | null | undefined): string | null {
  if (!platform) return null
  const known: Record<string, string> = {
    macos: `macOS`,
    darwin: `macOS`,
    linux: `Linux`,
    windows: `Windows`,
    win32: `Windows`,
  }
  return known[platform.toLowerCase()] ?? platform
}

/** EXP-409: a machine with no signed-in agent greys out. */
export function deviceHasRunnableAgent(device: DeviceListRow): boolean {
  return (device.agents ?? []).length > 0
}
