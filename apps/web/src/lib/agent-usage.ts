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

import { contract } from "@exp/domain-contract"
import type { SessionUsageState } from "@/lib/agent-feed"
import type {
  CodingSession,
  Device,
  DeviceAgentAccount,
  DeviceAgentHealth,
  DeviceAgentProfileEntry,
  DeviceAgentUsage,
  DeviceAgentUsageMap,
  DeviceUsageWindow,
} from "@/db/schema"
import type { CodingSessionBlocked } from "@exp/db-schema/domain"

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

/** EXP-849: belt-and-braces against a RETIRED agent id (`pi`) still sitting
 * in a synced row. The server clamps every write (lib/trpc/devices.ts), but
 * rows written before that clamp — or served by a self-hosted instance on an
 * older image — must still never produce a usage row, an account chip or a
 * health verdict for an agent this build has no name, icon or launcher for. */
export function isContractAgent(agent: string): boolean {
  return (contract.codingAgent.values as readonly string[]).includes(agent)
}

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

/** EXP-804: the one-line badge for a run's usage wall
 * (`coding_sessions.blocked`) — `Rate limited · resets in 2h`, or bare
 * `Rate limited` when the agent named no reset time. Null when the run is not
 * blocked, so a caller can render it beside the session state with a single
 * truthiness check.
 *
 * The wall is ORTHOGONAL to the session state: a blocked run still reads
 * `running`, so this NEVER replaces `sessionDisplayState` — it renders next
 * to it. An unrecognised `kind` still gets a badge (`Blocked`): a future
 * device reporting a wall this build has no name for must not render silent.
 */
export function blockedBadgeLabel(
  blocked: CodingSessionBlocked | null | undefined,
  now: Date
): string | null {
  if (!blocked) return null
  const kind = blocked.kind ?? `rate_limit`
  const label = kind === `rate_limit` ? `Rate limited` : `Blocked`
  const countdown = formatResetCountdown(blocked.resetsAt, now)
  return countdown ? `${label} · ${countdown}` : label
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

// ── The RUN's own context meter (EXP-746) ────────────────────────────────────
// Deliberately BESIDE `usageGroups`, never inside it: these numbers come from
// the ACP engine's `usage` activity event (tokens for one session), while the
// cards above come from the machine's rate-limit report (percentages for one
// account). Folding them together would break the ×4 `usageGroups` fixture
// lock and put a token count on a percent rail.

/** EXP-746 context section title. Byte-identical ×4. */
export const CONTEXT_SECTION_TITLE = `Context`

/** How full the run's context window is, 0-100, or null when the engine has
 * not measured one. Floored — a bar must never read 100% before it is.
 * MULTIPLY BEFORE DIVIDE: the other three clients compute `used * 100 / size`
 * in integers, and `(used / size) * 100` disagrees with them by one on exact
 * fractions (116000/200000 floors to 57 that way, 58 this way). */
export function contextPercent(
  usage: SessionUsageState | null | undefined
): number | null {
  if (!usage || usage.contextSize <= 0) return null
  const percent = Math.floor((usage.contextUsed * 100) / usage.contextSize)
  return Math.min(100, Math.max(0, percent))
}

/** k-rounded at >= 1000, no decimals — `124k`, `999`. Rounded, never
 * truncated: `1500` reads `2k` on all four clients. */
function formatTokens(value: number): string {
  return value >= 1000 ? `${Math.round(value / 1000)}k` : `${Math.round(value)}`
}

/** `124k / 200k (62%)` — empty when the engine reported no window (the
 * caller renders nothing at all). Byte-identical ×4. */
export function formatContextUsage(
  usage: SessionUsageState | null | undefined
): string {
  const percent = contextPercent(usage)
  if (!usage || percent === null) return ``
  return `${formatTokens(usage.contextUsed)} / ${formatTokens(usage.contextSize)} (${percent}%)`
}

/** EXP-850 §10: the session header's Context pill — `124k / 200k`, the same
 * numbers as `formatContextUsage` without the percent (the pill is small and
 * the sheet behind it spells the rest out). Empty when the engine reported no
 * window, which is what HIDES the pill. */
export function formatContextCompact(
  usage: SessionUsageState | null | undefined
): string {
  if (!usage || contextPercent(usage) === null) return ``
  return `${formatTokens(usage.contextUsed)} / ${formatTokens(usage.contextSize)}`
}

/** `$1.24`, or null under half a cent — a run that has spent essentially
 * nothing says nothing rather than `$0.00`. Byte-identical ×4. */
export function formatUsageCost(
  usage: SessionUsageState | null | undefined
): string | null {
  const cost = usage?.costUsd
  if (typeof cost !== `number` || !Number.isFinite(cost)) return null
  if (cost < 0.005) return null
  return `$${cost.toFixed(2)}`
}

/** What one agent's sign-in reads as. EXP-694 reduced it to the identity
 * alone: the bare email (no `signed in as` prefix and no ` · <plan>` tail —
 * the row's context already says both), the bare plan for an account with no
 * email (some agents report a provider, never an address), `signed in`,
 * `signed out`,
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
 * account with no email (a provider-only report) falls back
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

// ── EXP-849: account HEALTH ─────────────────────────────────────────────────
// `auth status` answers WHO a login is (email, plan) and nothing about
// whether it still works; the device's usage probe answers that (an
// Unauthorized probe is a dead credential, a successful one a live account)
// and writes it onto the account + every profile as `health`. Everything
// below is the PRESENTATION of that field, hand-mirrored with the desktop's
// `coding::agent_accounts` + `ui/src/usage_bar.rs` and the two natives'
// account rows: same four values, same fallback, same two badge strings.

/** What a row with no `health` at all means (a pre-EXP-849 device): the only
 * thing its payload says is whether the CLI was signed in. */
export function derivedAgentHealth(signedIn: boolean): DeviceAgentHealth {
  return signedIn ? `ok` : `signed_out`
}

/** One account's (or profile's) health: the device's own verdict when it sent
 * one, else derived from `signedIn`. A `signed_out` claim from a signed-IN
 * report is kept — the device is the authority on its own credential. */
export function agentHealth(
  entry:
    | Pick<DeviceAgentAccount, `signedIn` | `health`>
    | Pick<DeviceAgentProfileEntry, `signedIn` | `health`>
    | null
    | undefined
): DeviceAgentHealth {
  if (!entry) return `unknown`
  if (entry.health) return entry.health
  return derivedAgentHealth(entry.signedIn === true)
}

/** The badge an account row carries, or null when there is nothing to say
 * (`ok`, and `unknown` — "signed in, never probed" is not a problem). The two
 * negatives are DISTINCT on purpose: "Signed out" is a login you never made,
 * "Needs re-login" one that expired under you. */
export function healthBadgeLabel(
  health: DeviceAgentHealth
): `Needs re-login` | `Signed out` | null {
  if (health === `needs_relogin`) return `Needs re-login`
  if (health === `signed_out`) return `Signed out`
  return null
}

/** Worst-first ordering of the four values: an expired credential is the one
 * thing a human has to act on, a missing login next, then a login nobody has
 * probed, then a working one. */
export function healthRank(health: DeviceAgentHealth): number {
  if (health === `needs_relogin`) return 0
  if (health === `signed_out`) return 1
  if (health === `unknown`) return 2
  return 3
}

/** The worst health in a set — what a DEVICE row badges (its accounts' worst)
 * and what an account group shows across its machines. Null for an empty
 * set: nothing was reported, so nothing is claimed. */
export function worstHealth(
  healths: readonly DeviceAgentHealth[]
): DeviceAgentHealth | null {
  let worst: DeviceAgentHealth | null = null
  for (const health of healths) {
    if (worst === null || healthRank(health) < healthRank(worst)) worst = health
  }
  return worst
}

/** EXP-849: one account a MACHINE holds, for the device row's chips. The
 * Devices surface is the repair surface, so every chip names the agent, the
 * login, whether it is that machine's ACTIVE one and how healthy it is — the
 * account-level view (`agentProfileUsageRows`) groups ACROSS machines and is
 * the wrong shape for "what is wrong on this box". */
export interface DeviceAccountChip {
  /** `${agent}:${profileId}` — stable within one device row. */
  key: string
  agent: string
  profileId: string
  /** The profile's label (`Default` for the ambient login). */
  profileLabel: string
  signedIn: boolean
  active: boolean
  email: string | null
  plan: string | null
  health: DeviceAgentHealth
}

/** Every account one machine reported, agent by agent (contract order is the
 * caller's business), the ACTIVE login of each agent first. A machine that
 * reported no profiles yields its single ambient account. */
export function deviceAccountChips(
  device: HealthDeviceRow
): DeviceAccountChip[] {
  const out: DeviceAccountChip[] = []
  for (const [agent, account] of Object.entries(device.agentAccounts ?? {})) {
    if (!account || !isContractAgent(agent)) continue
    const profiles = (account.profiles ?? []).filter(
      (profile): profile is NonNullable<typeof profile> => Boolean(profile?.id)
    )
    if (profiles.length === 0) {
      out.push({
        key: `${agent}:${SYSTEM_PROFILE_ID}`,
        agent,
        profileId: SYSTEM_PROFILE_ID,
        profileLabel: `Default`,
        signedIn: account.signedIn === true,
        active: true,
        email: account.email || null,
        plan: account.plan || null,
        health: agentHealth(account),
      })
      continue
    }
    const chips = profiles.map((profile) => ({
      key: `${agent}:${profile.id}`,
      agent,
      profileId: profile.id,
      profileLabel:
        profile.label ||
        (profile.id === SYSTEM_PROFILE_ID ? `Default` : profile.id),
      signedIn: profile.signedIn === true,
      active: profile.active === true,
      email: profile.email || null,
      plan: profile.plan || null,
      health: agentHealth(profile),
    }))
    chips.sort((a, b) => Number(b.active) - Number(a.active))
    out.push(...chips)
  }
  return out
}

/** The three fields the chip rule reads — a `DeviceAccountChip` and the
 * account page's `AgentProfileUsageRow` both satisfy it. */
export type ChipActionRow = {
  signedIn: boolean
  active: boolean
  health: DeviceAgentHealth
}

/** EXP-849: whether the chip's lead entry is the non-destructive active-login
 * pick (`agent_profile_use`) rather than a sign-in. An EXPIRED credential is
 * never switched to — it would not work, it gets re-signed-in instead.
 *
 * `canSwitchAccount` is the MACHINE's `account-switch` capability
 * (`deviceCanSwitchAccount`, desktop/CLI ≥ 0.14.38). The server refuses
 * `agent_profile_use` without it (lib/trpc/devices.ts), so a machine below
 * that build must fall through to the sign-in action instead of being offered
 * a switch that is guaranteed to be refused. Mirrored ×3 (iOS/Android
 * `AgentAccountsRows.chipSwitchesTo`, desktop `accounts_section.rs`). */
export function chipSwitchesTo(
  row: ChipActionRow,
  canSwitchAccount: boolean
): boolean {
  return (
    canSwitchAccount &&
    row.signedIn &&
    !row.active &&
    row.health !== `needs_relogin`
  )
}

/** EXP-849: the ONE repair a MACHINE owes a login, as its chip menu's lead
 * entry — a healthy login the machine is not using simply BECOMES its login
 * (no login flow, no logout, no credential touched); everything else is a
 * sign-in. Byte-identical with iOS/Android `AgentAccountsRows.chipAction`. */
export function chipAction(
  row: ChipActionRow,
  canSwitchAccount: boolean
): string {
  if (!row.signedIn) return `Sign in`
  if (row.health === `needs_relogin`) return `Re-login`
  return chipSwitchesTo(row, canSwitchAccount)
    ? `Use this account here`
    : `Sign in again`
}

// `Device`'s column is nullable; `SteerDevice`'s is optional — accept both,
// so a device row and a composed machine can be passed unchanged.
type HealthDeviceRow = {
  agentAccounts?: Device[`agentAccounts`] | undefined
}

/** EXP-849: the health a DEVICE row badges — the worst among every account
 * it reported (each agent's profiles, or the top-level account for a
 * pre-profile machine). Null when the machine reported no account at all. */
export function deviceWorstHealth(
  device: HealthDeviceRow
): DeviceAgentHealth | null {
  const healths: DeviceAgentHealth[] = []
  for (const [agent, account] of Object.entries(device.agentAccounts ?? {})) {
    if (!account || !isContractAgent(agent)) continue
    const profiles = (account.profiles ?? []).filter(Boolean)
    if (profiles.length === 0) {
      healths.push(agentHealth(account))
      continue
    }
    for (const profile of profiles) healths.push(agentHealth(profile))
  }
  return worstHealth(healths)
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

// ── The cross-device usage page (EXP-792, EXP-747 C1-C4) ────────────────────
// One row per device × agent profile off the synced devices rows. Profiles
// (`agentAccounts[agent].profiles`, EXP-747 B5) carry their own usage; a
// device that reports none (an older build) falls back to the top-level
// account + `agentUsage[agent]` as the single `system` row, so the page never
// goes blank on a pre-profile machine.
//
// EXP-807: this section is mirrored on the DESKTOP — same names, same
// fallbacks, same ordering — in `apps/desktop/crates/ui/src/usage_bar.rs`
// (`agent_profile_usage_rows` / `peak_percent` / `attention_rank` /
// `sort_attention_first` / `refresh_allowed_at`), rendered by its
// `usage_view.rs`. A web+desktop PAIR, not the ×4 rule above it: iOS and
// Android ship no usage page (`packages/view-catalog/views.json`), only the
// per-run sheet the cards feed. Change a rule here, change it there.

/** The ambient login's profile id — byte-identical with the desktop's
 * `agent_profiles::SYSTEM_PROFILE`. */
export const SYSTEM_PROFILE_ID = `system`

/** A forced usage refresh (`agent_usage_refresh`) is refused while the last
 * fetch is younger than this: the device never hits the agent's usage
 * endpoint more often (its `RATE_LIMITED_FLOOR_SECS`), so the button greys
 * out and names the next allowed time instead of queueing a no-op. */
export const RATE_LIMITED_FLOOR_MS = 5 * 60 * 1000

export interface AgentProfileUsageRow {
  /** `${deviceId}:${agent}:${profileId}` — stable enough to key a list. */
  key: string
  deviceId: string
  deviceLabel: string
  /** Whether the row is one of the caller's own machines (a refresh is
   * only ever queued on those). */
  mine: boolean
  online: boolean
  agent: string
  profileId: string
  /** The profile's label (`Default` for the system profile when the device
   * sent none). */
  profileLabel: string
  active: boolean
  signedIn: boolean
  /** EXP-849: the device's verdict on the credential (`agentHealth`, derived
   * from `signedIn` on a pre-EXP-849 machine). */
  health: DeviceAgentHealth
  email: string | null
  plan: string | null
  usage: DeviceAgentUsage | null
  /** EXP-849: the device collects NO numbers for this login (it sits past
   * the machine's own probe cap). The row captions itself instead of
   * rendering absent bars as zero. */
  unmonitored: boolean
  /** The "as of …" fallback when the usage is stale or absent. */
  checkedAt: string | null
}

type UsageDeviceRow = Pick<
  Device,
  | `deviceId`
  | `label`
  | `userId`
  | `agentAccounts`
  | `agentUsage`
  | `agentUsageAt`
  | `lastSeenAt`
>

/** The rows the usage page renders for `devices`, grouped later by agent.
 * `online` is decided by the caller's clock the same way every device list
 * does (`deviceRowIsOnline`); it is passed in rather than re-derived so the
 * derivation stays a pure function of the rows. */
export function agentProfileUsageRows(
  devices: readonly UsageDeviceRow[],
  currentUserId: string,
  isOnline: (lastSeenAt: Date | string) => boolean
): AgentProfileUsageRow[] {
  const out: AgentProfileUsageRow[] = []
  for (const device of devices) {
    const accounts = device.agentAccounts ?? {}
    const usageMap = parseAgentUsageMap(device.agentUsage ?? {})
    const agents = new Set<string>(
      [...Object.keys(accounts), ...Object.keys(usageMap)].filter(
        isContractAgent
      )
    )
    for (const agent of agents) {
      const account = accounts[agent] ?? null
      const base = {
        deviceId: device.deviceId,
        deviceLabel: device.label,
        mine: device.userId === currentUserId,
        online: isOnline(device.lastSeenAt),
        agent,
      }
      const profiles = account?.profiles ?? []
      if (profiles.length === 0) {
        out.push({
          ...base,
          key: `${device.deviceId}:${agent}:${SYSTEM_PROFILE_ID}`,
          profileId: SYSTEM_PROFILE_ID,
          profileLabel: `Default`,
          active: true,
          signedIn: account?.signedIn === true,
          health: agentHealth(account),
          email: account?.email || null,
          plan: account?.plan || null,
          usage: usageMap[agent] ?? null,
          // A machine that reports no profiles reports one login, and it is
          // always inside its own probe cap.
          unmonitored: false,
          checkedAt:
            account?.checkedAt ??
            (device.agentUsageAt
              ? new Date(device.agentUsageAt).toISOString()
              : null),
        })
        continue
      }
      for (const profile of profiles) {
        // The active profile's numbers ride BOTH the profile entry and the
        // pre-profile `agentUsage[agent]` slot; prefer the profile's own and
        // fall back for a device that only populated the old slot.
        const usage =
          parseAgentUsage(profile.usage) ??
          (profile.active ? (usageMap[agent] ?? null) : null)
        out.push({
          ...base,
          key: `${device.deviceId}:${agent}:${profile.id}`,
          profileId: profile.id,
          profileLabel:
            profile.label ||
            (profile.id === SYSTEM_PROFILE_ID ? `Default` : profile.id),
          active: profile.active === true,
          signedIn: profile.signedIn === true,
          health: agentHealth(profile),
          email: profile.email || null,
          plan: profile.plan || null,
          usage,
          unmonitored: profile.unmonitored === true,
          checkedAt: profile.checkedAt ?? account?.checkedAt ?? null,
        })
      }
    }
  }
  return out
}

/** EXP-862: what an account row has to SAY about its numbers, so no surface
 * has to invent a caption for an empty usage list:
 *  - `ready` — numbers to render (stale or not: freshness is the bar's own
 *    business, `usageIsFresh`);
 *  - `checking` — a signed-in, monitored login this machine has not read yet.
 *    Every login is read now (the first read skips the rotation queue), so
 *    this is a beat or two, not a permanent state;
 *  - `unmonitored` — the machine deliberately collects nothing for it (past
 *    its own probe cap);
 *  - `none` — nothing to report at all: the login is signed out, and its row
 *    offers a sign-in instead of a bar.
 *
 * Mirrored on the desktop (`usage_bar.rs`, the web+desktop pair above). */
export type UsageState = `ready` | `checking` | `unmonitored` | `none`

export function usageState(
  row: Pick<AgentProfileUsageRow, `signedIn` | `unmonitored` | `usage`>
): UsageState {
  if (!row.signedIn) return `none`
  if (row.unmonitored) return `unmonitored`
  return (row.usage?.windows.length ?? 0) > 0 ? `ready` : `checking`
}

/** The fullest window's percent, or 0 for a row with no usage at all. */
export function peakPercent(usage: DeviceAgentUsage | null | undefined): number {
  let peak = 0
  for (const window of usage?.windows ?? []) {
    if (window.percent > peak) peak = window.percent
  }
  return peak
}

/** Attention-first ordering: signed-out rows lead (there is something to do),
 * then rows at or over `DANGER_PERCENT`, then everything else. Within a
 * bucket the fuller row comes first, then device label, agent, profile, so
 * a heartbeat cannot shuffle equal rows. */
export function attentionRank(
  row: Pick<AgentProfileUsageRow, `signedIn` | `usage`> & {
    health?: DeviceAgentHealth
  }
): number {
  if (!row.signedIn) return 0
  // EXP-849: an EXPIRED credential is the same kind of "do something" as a
  // missing one — it leads too, even though the CLI still reports signed in.
  if (row.health === `needs_relogin`) return 0
  if (peakPercent(row.usage) >= DANGER_PERCENT) return 1
  return 2
}

export function sortAttentionFirst(
  rows: readonly AgentProfileUsageRow[]
): AgentProfileUsageRow[] {
  return [...rows].sort((a, b) => {
    const byRank = attentionRank(a) - attentionRank(b)
    if (byRank !== 0) return byRank
    const byPeak = peakPercent(b.usage) - peakPercent(a.usage)
    if (byPeak !== 0) return byPeak
    const byDevice = a.deviceLabel.localeCompare(b.deviceLabel)
    if (byDevice !== 0) return byDevice
    const byAgent = a.agent.localeCompare(b.agent)
    if (byAgent !== 0) return byAgent
    return a.profileId.localeCompare(b.profileId)
  })
}

/** When a forced refresh is next allowed for `usage`: null = right now (no
 * fetch on record, or the last one is older than the floor). A stamp in the
 * future (the machine's clock runs ahead) is treated as "just fetched". */
export function refreshAllowedAt(
  usage: DeviceAgentUsage | null | undefined,
  now: Date
): Date | null {
  if (!usage?.fetchedAt) return null
  const fetched = new Date(usage.fetchedAt).getTime()
  if (Number.isNaN(fetched)) return null
  const next = fetched + RATE_LIMITED_FLOOR_MS
  return next > now.getTime() ? new Date(next) : null
}

// ── EXP-817: one card per ACCOUNT ───────────────────────────────────────────
// The page rows above are device × profile; the same login on two machines
// reported the same numbers twice (and, before the poll-pin fix, at two
// different ages). So the page renders one card per ACCOUNT — an agent plus
// the email the machine named — and lists the machines that hold it as
// chips. Mirrored on the desktop (`usage_bar.rs`, same names, same tests).

export interface AgentAccountUsageGroup {
  /** `${agent}:${email}` for a named login; a row with no email (a
   * provider-only report, or a signed-out row) can never be told apart from
   * another machine's, so it keeps its own `${agent}:${deviceId}:${profileId}`. */
  key: string
  agent: string
  signedIn: boolean
  /** EXP-849: the WORST health among the machines holding this account — the
   * badge the account row carries (one machine's expired credential is a
   * re-login, even if another machine's copy still works). */
  health: DeviceAgentHealth
  email: string | null
  plan: string | null
  /** The machines (× profile) holding this account: online first, then by
   * label, then profile — a heartbeat cannot reshuffle the chips. */
  rows: AgentProfileUsageRow[]
  /** The FRESHEST member's numbers: newest `fetchedAt`, a non-stale report
   * winning a tie, a report with windows beating one without. */
  usage: DeviceAgentUsage | null
  /** The newest probe stamp among the members — the "as of …" fallback. */
  checkedAt: string | null
  /** Where a refresh is queued: the eligible member (`canRefresh`) that
   * reported the freshest numbers, or null when no member may run one. */
  refreshTarget: AgentProfileUsageRow | null
}

function stampMs(stamp: string | null | undefined): number {
  if (!stamp) return Number.NEGATIVE_INFINITY
  const ms = new Date(stamp).getTime()
  return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms
}

/** Whether `candidate` is a fresher report than `current`. */
function fresherUsage(
  candidate: DeviceAgentUsage | null,
  current: DeviceAgentUsage | null
): boolean {
  if (!candidate) return false
  if (!current) return true
  const byStamp = stampMs(candidate.fetchedAt) - stampMs(current.fetchedAt)
  if (byStamp !== 0) return byStamp > 0
  if (candidate.stale !== current.stale) return current.stale === true
  return candidate.windows.length > 0 && current.windows.length === 0
}

export function accountGroupKey(row: AgentProfileUsageRow): string {
  const email = row.signedIn ? row.email?.trim().toLowerCase() : null
  return email
    ? `${row.agent}:${email}`
    : `${row.agent}:${row.deviceId}:${row.profileId}`
}

/** Fold the page rows into account groups. `canRefresh` decides which
 * members may run `agent_usage_refresh` (mine + online + the cap) — injected
 * so the derivation stays a pure function of the rows. Nothing is sorted
 * across groups here: `sortAccountGroupsAttentionFirst` owns that. */
export function accountUsageGroups(
  rows: readonly AgentProfileUsageRow[],
  canRefresh: (row: AgentProfileUsageRow) => boolean
): AgentAccountUsageGroup[] {
  const byKey = new Map<string, AgentAccountUsageGroup>()
  for (const row of rows) {
    const key = accountGroupKey(row)
    let group = byKey.get(key)
    if (!group) {
      group = {
        key,
        agent: row.agent,
        signedIn: row.signedIn,
        health: row.health,
        email: row.email,
        plan: row.plan,
        rows: [],
        usage: null,
        checkedAt: null,
        refreshTarget: null,
      }
      byKey.set(key, group)
    }
    group.rows.push(row)
    if (healthRank(row.health) < healthRank(group.health)) {
      group.health = row.health
    }
    if (!group.plan && row.plan) group.plan = row.plan
    if (fresherUsage(row.usage, group.usage)) group.usage = row.usage
    if (stampMs(row.checkedAt) > stampMs(group.checkedAt)) {
      group.checkedAt = row.checkedAt
    }
    if (
      canRefresh(row) &&
      (!group.refreshTarget || fresherUsage(row.usage, group.refreshTarget.usage))
    ) {
      group.refreshTarget = row
    }
  }
  for (const group of byKey.values()) {
    group.rows.sort((a, b) => {
      if (a.online !== b.online) return a.online ? -1 : 1
      const byDevice = a.deviceLabel.localeCompare(b.deviceLabel)
      if (byDevice !== 0) return byDevice
      return a.profileId.localeCompare(b.profileId)
    })
  }
  return [...byKey.values()]
}

/** `attentionRank` over groups, then the fuller group, then agent, then the
 * key (email or device) — the page order. */
export function sortAccountGroupsAttentionFirst(
  groups: readonly AgentAccountUsageGroup[]
): AgentAccountUsageGroup[] {
  return [...groups].sort((a, b) => {
    const byRank = attentionRank(a) - attentionRank(b)
    if (byRank !== 0) return byRank
    const byPeak = peakPercent(b.usage) - peakPercent(a.usage)
    if (byPeak !== 0) return byPeak
    const byAgent = a.agent.localeCompare(b.agent)
    if (byAgent !== 0) return byAgent
    return a.key.localeCompare(b.key)
  })
}
