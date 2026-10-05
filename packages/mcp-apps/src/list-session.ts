import type { RunDetail } from "./model"
import { relativeTime } from "./time"

// EXP-1183 — the session list's pure half, ported from the web's
// `lib/coding-session-display.ts`, `lib/session-identity.ts`,
// `lib/past-runs.ts` and `lib/sessions/session-tree.ts` (the ×4 rules) onto
// the `exponential_sessions_list` wire row. The views never import apps/web.

/** The `exponential_sessions_list` row fields the list reads beyond
 *  `RunDetail` (tools.ts `sessionColumns`). */
export interface SessionListRow extends RunDetail {
  deviceLabel?: string | null
  parentSessionId?: string | null
  resumedFromId?: string | null
  startedAt?: string | null
  endedAt?: string | null
  updatedAt?: string | null
  blocked?: { kind?: string | null; resetsAt?: string | null } | null
}

/** `fixtures/session-display.json`: what a LIVE run is doing. */
export type SessionDisplayState = `needs_input` | `working` | `review` | `done`

export function sessionDisplayState(row: SessionListRow): SessionDisplayState {
  if (row.needsInput) return `needs_input`
  if (row.agentBusy) return `working`
  const prOpen = row.prState !== `merged` && row.prState !== `closed`
  return row.status === `in_review` && prOpen ? `review` : `done`
}

/** EXP-848: only a live row mid-turn animates. */
export function sessionRowIsWorking(row: SessionListRow): boolean {
  return row.status !== `ended` && sessionDisplayState(row) === `working`
}

/** EXP-850 §8: the device-written caption, live rows only. */
export function sessionAgentCaption(row: SessionListRow): string | null {
  if (row.status === `ended` || row.status === `merged`) return null
  const caption = row.agentCaption?.trim()
  return caption ? caption : null
}

/** The web's `formatDistanceToNowStrict` wording, compact: `5m ago`,
 *  `just now`, or the date beyond a week (`relativeTime`). */
export function ago(value: string | null | undefined, now = Date.now()): string {
  const short = relativeTime(value ?? undefined, now)
  if (!short) return ``
  if (short === `now`) return `just now`
  return /^\d+[mhd]$/.test(short) ? `${short} ago` : short
}

export type SessionStatusTone = `muted` | `amber` | `emerald` | `sky`

export const SESSION_STATUS_TONE_CLASS: Record<SessionStatusTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

/** The device a row names (`device.label || session.deviceLabel || 'Desktop'`
 *  — the MCP row carries only the snapshot). */
export function sessionDeviceName(row: SessionListRow): string {
  return row.deviceLabel?.trim() || `Desktop`
}

/** EXP-874: a LIVE row's status line — the parked state, then the machine. */
export function sessionStatusLine(
  row: SessionListRow,
  now = Date.now()
): { text: string; tone: SessionStatusTone } {
  const device = sessionDeviceName(row)
  switch (sessionDisplayState(row)) {
    case `needs_input`:
      return { text: `Needs input · ${device}`, tone: `amber` }
    case `review`:
      return { text: `Ready for review · ${device}`, tone: `emerald` }
    case `done`:
      return { text: `Done · ${device}`, tone: `sky` }
    case `working`: {
      const started = ago(row.startedAt ?? row.createdAt, now)
      return {
        text: started ? `${device} · started ${started}` : device,
        tone: `muted`,
      }
    }
  }
}

/** EXP-746: an ended row's byline — `<device> · <when it ended>`. */
export function pastRunByline(row: SessionListRow, now = Date.now()): string {
  const when = ago(row.endedAt ?? row.updatedAt, now)
  return [row.deviceLabel?.trim(), when].filter(Boolean).join(` · `)
}

/** EXP-804: the usage wall, `Rate limited · resets in 2h`. */
export function blockedBadgeLabel(
  blocked: SessionListRow[`blocked`],
  now = Date.now()
): string | null {
  if (!blocked) return null
  const label = (blocked.kind ?? `rate_limit`) === `rate_limit` ? `Rate limited` : `Blocked`
  const countdown = resetCountdown(blocked.resetsAt, now)
  return countdown ? `${label} · ${countdown}` : label
}

/** `resets in 45m` / `resets in 2h 10m` / `resets in 3d 14h` (agent-usage.ts). */
export function resetCountdown(
  resetsAt: string | null | undefined,
  now = Date.now()
): string | null {
  if (!resetsAt) return null
  const at = new Date(resetsAt).getTime()
  if (Number.isNaN(at)) return null
  const ms = at - now
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

/** EXP-688: the mono identifier (issue runs) and the human subject. The MCP
 *  row has no batch issues or agent title, so a batch reads `Batch run`. */
export function sessionIdentity(row: SessionListRow): {
  identifier: string | null
  subject: string
} {
  if (row.issueIdentifier) {
    return {
      identifier: row.issueIdentifier,
      subject: row.issueTitle?.trim() || `Untitled issue`,
    }
  }
  if (row.actionName) return { identifier: null, subject: row.actionName }
  if (row.issueId) return { identifier: null, subject: `Issue syncing…` }
  return { identifier: null, subject: `Batch run` }
}

export type SessionBand = `running` | `in_review` | `ended`

export const SESSION_BAND_LABEL: Record<SessionBand, string> = {
  running: `Running`,
  in_review: `In review`,
  ended: `Ended`,
}

const BAND_ORDER: readonly SessionBand[] = [`running`, `in_review`, `ended`]

function bandOf(row: SessionListRow): SessionBand {
  if (row.status === `running`) return `running`
  if (row.status === `in_review`) return `in_review`
  return `ended`
}

/** One drawn row of a band: how deep it nests and whether it folds. */
export interface SessionTreeRow {
  row: SessionListRow
  depth: number
  hasChildren: boolean
}

export interface SessionBandGroup {
  band: SessionBand
  label: string
  /** Every row in the band's tree, collapsed ones included. */
  count: number
  rows: SessionTreeRow[]
}

function stamp(value: string | null | undefined): number {
  if (!value) return 0
  const ms = new Date(value).getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/**
 * EXP-1183 — the run list as the app's session tree (`session-tree.ts`):
 * a resumed run collapses into its newest successor, a child nests under its
 * `parentSessionId` when that run is in the list (else it sits at top level),
 * children keep creation order and roots keep the tool's order (newest
 * first). The ROOT decides the band, so a tree never splits across bands.
 * `collapsed` = root/parent ids whose children are folded away.
 */
export function sessionBands(
  rows: readonly SessionListRow[],
  collapsed: ReadonlySet<string> = new Set()
): SessionBandGroup[] {
  // Resume successions: a row some other listed row resumed is folded into
  // that successor (the newest wins the walk).
  const successorOf = new Map<string, string>()
  for (const row of rows) {
    if (row.resumedFromId && row.resumedFromId !== row.id) {
      successorOf.set(row.resumedFromId, row.id)
    }
  }
  const canonical = (id: string): string => {
    const seen = new Set<string>()
    let cursor = id
    while (successorOf.has(cursor) && !seen.has(cursor)) {
      seen.add(cursor)
      cursor = successorOf.get(cursor)!
    }
    return cursor
  }
  const visible = rows.filter((row) => canonical(row.id) === row.id)
  const byId = new Map(visible.map((row) => [row.id, row]))

  const parentOf = new Map<string, string>()
  for (const row of visible) {
    const parent = row.parentSessionId ? canonical(row.parentSessionId) : null
    if (parent && parent !== row.id && byId.has(parent)) parentOf.set(row.id, parent)
  }
  // A cycle (never written by the server) leaves the closing row at the top.
  const isRoot = (id: string): boolean => {
    const seen = new Set<string>([id])
    let cursor = parentOf.get(id)
    while (cursor) {
      if (seen.has(cursor)) return true
      seen.add(cursor)
      cursor = parentOf.get(cursor)
    }
    return !parentOf.has(id)
  }
  const children = new Map<string, SessionListRow[]>()
  const roots: SessionListRow[] = []
  for (const row of visible) {
    const parent = parentOf.get(row.id)
    if (!parent || isRoot(row.id)) {
      roots.push(row)
      continue
    }
    const list = children.get(parent) ?? []
    list.push(row)
    children.set(parent, list)
  }
  for (const list of children.values()) {
    list.sort(
      (a, b) =>
        stamp(a.createdAt) - stamp(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    )
  }

  return BAND_ORDER.map((band) => {
    const out: SessionTreeRow[] = []
    let count = 0
    const walk = (row: SessionListRow, depth: number, drawn: boolean) => {
      count += 1
      const kids = children.get(row.id) ?? []
      if (drawn) out.push({ row, depth, hasChildren: kids.length > 0 })
      const open = drawn && !collapsed.has(row.id)
      for (const kid of kids) walk(kid, depth + 1, open)
    }
    for (const root of roots) if (bandOf(root) === band) walk(root, 0, true)
    return { band, label: SESSION_BAND_LABEL[band], count, rows: out }
  }).filter((group) => group.rows.length > 0)
}
