// EXP-484: how a machine's per-agent auth + usage status is PRESENTED. The
// device collects it locally (it never holds, copies or refreshes a
// credential) and ships it on register/heartbeat into `devices.agent_accounts`
// / `agent_usage`; every client then renders the same bars, captions and
// countdowns off the synced row.
//
// Hand-mirrored ×4 against the same fixture and the same test names:
//   iOS      apps/ios/ExpCore/Sources/Domain/AgentUsagePresentation.swift
//   Android  apps/android/.../domain/AgentUsagePresentation.kt
//   desktop  apps/desktop/crates/ui/src/usage_bar.rs
// Changing a rule or a string here means changing it in all four.
//
// EXP-688: every window the machine reports is SHOWN, grouped the way the
// agent's own app groups them (`usageGroups`). There is no pinned window and
// no "the fullest one" heuristic any more — a reading habit nobody had.

import type {
  CodingSession,
  Device,
  DeviceAgentAccount,
  DeviceAgentUsage,
  DeviceAgentUsageMap,
  DeviceUsageWindow,
} from "@/db/schema"

/** Usage numbers older than this are STALE: the bar dims and captions itself
 * `as of <relative>` instead of claiming to be current. Fails closed — a
 * missing or unparsable `fetchedAt` is never fresh. */
export const USAGE_FRESH_MS = 15 * 60 * 1000

/** ≥ this percent reads as warning (amber), ≥ `DANGER_PERCENT` as danger. */
export const WARNING_PERCENT = 75
export const DANGER_PERCENT = 95

/** At most this many windows render — the device already clamps to it
 * (lib/trpc/devices.ts); the parser holds the line for rows written before a
 * clamp existed. */
export const MAX_USAGE_WINDOWS = 10

export type UsageSeverity = `normal` | `warning` | `danger`

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

function parseWindow(value: unknown): DeviceUsageWindow | null {
  if (!isRecord(value)) return null
  const key = typeof value.key === `string` ? value.key : ``
  const label = typeof value.label === `string` ? value.label : ``
  if (key.length === 0 || label.length === 0) return null
  const raw = typeof value.percent === `number` ? value.percent : 0
  const percent = Number.isFinite(raw)
    ? Math.min(100, Math.max(0, Math.round(raw)))
    : 0
  const resetsAt = typeof value.resetsAt === `string` ? value.resetsAt : null
  return { key, label, percent, resetsAt }
}

/** Tolerant parse of ONE agent's usage entry off the synced jsonb: unknown
 * fields ride along unread, malformed windows drop, and anything that isn't
 * an object at all yields null. Never throws — a client must not brick on a
 * newer device's payload. */
export function parseAgentUsage(value: unknown): DeviceAgentUsage | null {
  if (!isRecord(value)) return null
  const windows: DeviceUsageWindow[] = []
  if (Array.isArray(value.windows)) {
    for (const entry of value.windows) {
      if (windows.length >= MAX_USAGE_WINDOWS) break
      const window = parseWindow(entry)
      if (window) windows.push(window)
    }
  }
  const usage: DeviceAgentUsage = { windows }
  if (typeof value.fetchedAt === `string`) usage.fetchedAt = value.fetchedAt
  if (typeof value.stale === `boolean`) usage.stale = value.stale
  return usage
}

/** Tolerant parse of the whole per-agent map. Entries that parse to null are
 * dropped; a non-object input yields an empty map. */
export function parseAgentUsageMap(value: unknown): DeviceAgentUsageMap {
  if (!isRecord(value)) return {}
  const out: DeviceAgentUsageMap = {}
  for (const [agent, entry] of Object.entries(value)) {
    const usage = parseAgentUsage(entry)
    if (usage) out[agent] = usage
  }
  return out
}

/** Fresh = fetched within `USAGE_FRESH_MS`. FAIL-CLOSED: a missing or
 * unparsable `fetchedAt` is never fresh; a stamp in the future (the machine's
 * clock runs ahead) is. The device's own `stale` flag (an expired credential,
 * a 401, a failed fetch with the older numbers kept) is a SEPARATE dimming
 * input the view ORs in — it does not decide freshness. */
export function usageIsFresh(
  usage: DeviceAgentUsage | null | undefined,
  now: Date
): boolean {
  if (!usage) return false
  if (!usage.fetchedAt) return false
  const fetched = new Date(usage.fetchedAt).getTime()
  if (Number.isNaN(fetched)) return false
  return now.getTime() - fetched < USAGE_FRESH_MS
}

/** Tone thresholds — the same three everywhere. */
export function severity(percent: number): UsageSeverity {
  if (percent >= DANGER_PERCENT) return `danger`
  if (percent >= WARNING_PERCENT) return `warning`
  return `normal`
}

/** `resets in 45m` / `resets in 2h 10m` / `resets in 3d 14h`, and
 * `resets soon` inside the last minute or once the stamp has passed. Null
 * when the window carries no reset (the device could not read one). */
export function formatResetCountdown(
  resetsAt: string | null | undefined,
  now: Date
): string | null {
  if (!resetsAt) return null
  const at = new Date(resetsAt).getTime()
  if (Number.isNaN(at)) return null
  const ms = at - now.getTime()
  if (ms < 60_000) return `resets soon`
  const minutes = Math.floor(ms / 60_000)
  if (minutes < 60) return `resets in ${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    const rest = minutes % 60
    return rest === 0 ? `resets in ${hours}h` : `resets in ${hours}h ${rest}m`
  }
  const days = Math.floor(hours / 24)
  const rest = hours % 24
  return rest === 0 ? `resets in ${days}d` : `resets in ${days}d ${rest}h`
}

export type UsageGroupKey = `session` | `weekly` | `other`

/** One rendered window: what it is called, how full it is and the one line
 * under it. `percent` is already clamped by the parser. */
export interface UsageCard {
  /** The wire key — stable enough to render a list with. */
  key: string
  title: string
  percent: number
  severity: UsageSeverity
  /** `resets in 2h 10m`, `Starts when a message is sent`, or empty. */
  caption: string
}

export interface UsageGroup {
  key: UsageGroupKey
  /** EXP-694: empty means the group renders WITHOUT a heading — the weekly
   * group's cards ("All models", "<Model> only") already name themselves, and
   * a "Weekly limits" line above them was one label too many. Renderers skip
   * an empty title. */
  title: `Current session` | `` | `Other`
  cards: UsageCard[]
}

/** What ONE window is called: the agent apps name the five-hour window
 * "Current session", the rolling week "All models", and a per-model window
 * "<Model> only". Anything else (credits, codex's month) keeps the label the
 * machine sent. */
function cardTitle(window: DeviceUsageWindow): string {
  if (window.key === `session`) return `Current session`
  if (window.key === `weekly`) return `All models`
  if (window.key.startsWith(`model:`)) return `${window.label} only`
  return window.label
}

function cardCaption(window: DeviceUsageWindow, now: Date): string {
  const countdown = formatResetCountdown(window.resetsAt, now)
  if (countdown) return countdown
  // Claude's own app says this about an idle session window: it is not "0%
  // used", it has not started.
  if (window.key === `session` && window.percent === 0) {
    return `Starts when a message is sent`
  }
  return ``
}

/** EXP-688: every reported window, grouped the way the agent's own app groups
 * them — the current session, the weekly limits (all models first, then the
 * per-model ones in report order), then everything else in report order.
 * Empty groups are omitted; the group order is fixed. EXP-694: the weekly
 * group carries NO title. */
export function usageGroups(
  usage: DeviceAgentUsage | null | undefined,
  now: Date
): UsageGroup[] {
  const session: UsageCard[] = []
  const weekly: UsageCard[] = []
  const models: UsageCard[] = []
  const other: UsageCard[] = []
  for (const window of usage?.windows ?? []) {
    const card: UsageCard = {
      key: window.key,
      title: cardTitle(window),
      percent: window.percent,
      severity: severity(window.percent),
      caption: cardCaption(window, now),
    }
    if (window.key === `session`) session.push(card)
    else if (window.key === `weekly`) weekly.push(card)
    else if (window.key.startsWith(`model:`)) models.push(card)
    else other.push(card)
  }
  const groups: UsageGroup[] = []
  if (session.length > 0) {
    groups.push({ key: `session`, title: `Current session`, cards: session })
  }
  if (weekly.length > 0 || models.length > 0) {
    groups.push({
      key: `weekly`,
      title: ``,
      cards: [...weekly, ...models],
    })
  }
  if (other.length > 0) {
    groups.push({ key: `other`, title: `Other`, cards: other })
  }
  return groups
}

/** What one agent's sign-in reads as. EXP-694 reduced it to the identity
 * alone: the bare email (no `signed in as` prefix and no ` · <plan>` tail —
 * the row's context already says both), the bare plan for an account with no
 * email (pi reports a provider, never an address), `signed in`, `signed out`,
 * or `unknown` when the machine reported nothing for the agent (never probed
 * is not "signed out"). */
export function accountCaption(
  account: DeviceAgentAccount | null | undefined
): string {
  if (!account) return `unknown`
  if (!account.signedIn) return `signed out`
  const email = account.email && account.email.length > 0 ? account.email : null
  if (email) return email
  const plan = account.plan && account.plan.length > 0 ? account.plan : null
  if (plan) return plan
  return `signed in`
}

/** EXP-688/694: what one agent's OWN tab says, where the agent is already the
 * heading — just the ADDRESS: no `claude · ` prefix, no `signed in as` and no
 * ` · <plan>` tail (the plan is the agent app's business, not this row's). An
 * account with no email (pi reports a provider, never an address) falls back
 * to the bare plan, and the two negative cases read as sentences. */
export function accountLine(
  account: DeviceAgentAccount | null | undefined
): string {
  if (!account) return `Sign-in status unknown`
  if (!account.signedIn) return `Not signed in`
  const email = account.email && account.email.length > 0 ? account.email : null
  if (email) return email
  const plan = account.plan && account.plan.length > 0 ? account.plan : null
  if (plan) return plan
  return `signed in`
}

/** The whole row: `claude · danny@example.com`. */
export function accountRow(
  agent: string,
  account: DeviceAgentAccount | null | undefined
): string {
  return `${agent} · ${accountCaption(account)}`
}

type SessionUsageRow = Pick<
  CodingSession,
  `deviceId` | `userId` | `agent` | `status`
>
type SessionUsageDevice = Pick<Device, `deviceId` | `userId` | `agentUsage`>

/** The usage bar a coding session shows, or null when it shows none.
 *
 * Renders only for a run that is still going (`running` / `in_review`) on a
 * machine whose report is FRESH and non-empty for the run's OWN agent: a
 * finished run's host limits are nobody's business, and stale numbers beside
 * a live agent read as current ones. The devices-row join mirrors
 * `resolveSessionDevice` exactly (the stamped `device_id`, preferring the
 * session owner's own row — two users can see the same machine id through a
 * shared server row). */
export function sessionAgentUsage(
  session: SessionUsageRow,
  devices: readonly SessionUsageDevice[],
  now: Date
): { agent: string; usage: DeviceAgentUsage } | null {
  if (session.status !== `running` && session.status !== `in_review`) {
    return null
  }
  if (!session.agent || !session.deviceId) return null
  const matches = devices.filter(
    (device) => device.deviceId === session.deviceId
  )
  const row =
    matches.find((device) => device.userId === session.userId) ?? matches[0]
  if (!row) return null
  const usage = parseAgentUsage(row.agentUsage?.[session.agent])
  if (!usage || usage.windows.length === 0) return null
  if (!usageIsFresh(usage, now)) return null
  return { agent: session.agent, usage }
}

/** EXP-484: what the device wrote into a finished `agent_login` command's
 * `result`. The executor completes the row EARLY, the moment the agent CLI
 * puts a sign-in URL on the grid, with the JSON `LoginProgress` shape
 * (desktop `coding::agent_login::LoginProgress::to_result_text`); a failure
 * completes with `phase: "failed"` and a human message. Codex device-code
 * flows carry a `code`, claude's does not. */
export interface AgentLoginProgress {
  agent: string
  phase: `url` | `failed`
  url: string | null
  code: string | null
  message: string | null
}

/** Tolerant parse of that result string. Null for anything that isn't a
 * login answer at all — a worktree command's plain-text summary, an empty
 * result, malformed JSON, or a `url` phase with no URL — so the caller falls
 * back to rendering the raw text. */
export function parseAgentLoginResult(
  result: string | null | undefined
): AgentLoginProgress | null {
  if (!result) return null
  let value: unknown
  try {
    value = JSON.parse(result)
  } catch {
    return null
  }
  if (!isRecord(value)) return null
  const phase = value.phase
  if (phase !== `url` && phase !== `failed`) return null
  const url = typeof value.url === `string` && value.url.length > 0 ? value.url : null
  if (phase === `url` && !url) return null
  return {
    agent: typeof value.agent === `string` ? value.agent : ``,
    phase,
    url,
    code:
      typeof value.code === `string` && value.code.length > 0
        ? value.code
        : null,
    message:
      typeof value.message === `string` && value.message.length > 0
        ? value.message
        : null,
  }
}
