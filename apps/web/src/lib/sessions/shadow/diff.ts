// #EXP-1510: what the core decided vs what the server did, on the fields
// both model. Pure (unit-tested): the session rows the event touched and
// the lines it posted. Times are never compared (the core's `at` and the
// server's clock differ by milliseconds), only whether a stamp is set.

import type { Effect, HubMessage, HubSession, HubState, Outcome } from "@exp/eagent/model"

export interface ShadowDiff {
  /** The core refused (or accepted) where the server did the opposite. */
  refusal?: { core: string | null; server: string | null }
  sessions: Array<{ id: string; field: string; core: unknown; server: unknown }>
  /** Lines one side posted and the other did not (`to|author|from|card|waking[|#bodyhash]`). */
  posts: { coreOnly: string[]; serverOnly: string[] }
}

const FIELDS = [`status`, `busy`, `background`, `needsInput`, `parentId`, `rootId`, `depth`, `deviceId`, `shared`] as const

function view(row: HubSession | undefined): Record<string, unknown> | null {
  if (!row) return null
  const out: Record<string, unknown> = {}
  for (const field of FIELDS) out[field] = row[field]
  out.blocked = row.blocked ? `${row.blocked.kind}${row.blocked.resume === undefined ? `` : row.blocked.resume ? `:resume` : `:hold`}` : null
  out.wakeOn = [...row.wakeOn].sort().join(`,`)
  out.started = row.startedAt !== null
  out.ended = row.endedAt !== null
  return out
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

/** A posted line's identity; the body only when neither side may hold a sealed one. */
export function lineKey(line: {
  to: string
  authorKind: string
  authorSessionId: string | null
  card: HubMessage[`card`]
  body: string
  sealed: boolean
  waking: boolean
}): string {
  const card = line.card
    ? `${line.card.kind}${`event` in line.card && line.card.event ? `:${line.card.event}` : ``}`
    : `-`
  const parts = [line.to, line.authorKind, line.authorSessionId ?? `-`, card, line.waking ? `waking` : `quiet`]
  if (!line.sealed && line.body) parts.push(`#${bodyHash(line.body.trim())}`)
  return parts.join(`|`)
}

/** FNV-1a over UTF-16 units: bodies are compared, never stored (a sample is no transcript). */
function bodyHash(text: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193)
  return (h >>> 0).toString(16).padStart(8, `0`)
}

/** Waking, normalised for both sides: unconsumed INPUT for its reader (a person's line, or another session's;
 *  the server leaves a session's own rows unconsumed, the core calls them quiet). */
const input = (to: string, authorKind: string, authorSessionId: string | null) =>
  authorKind === `user` || (authorSessionId !== null && authorSessionId !== to)

/** The server's posted message as a core line. */
export const postedKey = (m: HubMessage) =>
  lineKey({ ...m, to: m.sessionId, waking: m.pending && input(m.sessionId, m.authorKind, m.authorSessionId) })

const coreKey = (e: Extract<Effect, { kind: `post` }>) =>
  lineKey({ ...e, waking: e.waking && input(e.to, e.authorKind, e.authorSessionId) })

function multisetDiff(a: string[], b: string[]): [string[], string[]] {
  const left = [...a]
  const right: string[] = []
  for (const key of b) {
    const at = left.indexOf(key)
    if (at >= 0) left.splice(at, 1)
    else right.push(key)
  }
  return [left, right]
}

/** `before` = the state the core reduced; `outcome` = its decision; `after` + `posted` = what the server left.
 *  Only rows that either side changed are compared (another event's concurrent write to an untouched row is no diff). */
export function diffShadow(input: {
  before: HubState
  outcome: Outcome
  after: HubState
  posted: HubMessage[]
  serverError: string | null
}): ShadowDiff {
  const diff: ShadowDiff = { sessions: [], posts: { coreOnly: [], serverOnly: [] } }
  const coreFailure = input.outcome.failure ? `${input.outcome.failure.code}: ${input.outcome.failure.message}` : null
  if ((coreFailure === null) !== (input.serverError === null)) diff.refusal = { core: coreFailure, server: input.serverError }
  if (coreFailure !== null || input.serverError !== null) return diff
  const ids = new Set([...Object.keys(input.outcome.state.sessions), ...Object.keys(input.after.sessions)])
  for (const id of [...ids].sort()) {
    const was = view(input.before.sessions[id])
    const core = view(input.outcome.state.sessions[id])
    const server = view(input.after.sessions[id])
    if (same(core, server)) continue
    if (!was && !core) continue // a row the core never knew of (another event created it)
    for (const field of new Set([...Object.keys(core ?? {}), ...Object.keys(server ?? {})])) {
      const c = core?.[field]
      const s = server?.[field]
      if (same(c, s)) continue
      // Untouched by the core and moved by the server only through another event? Still a diff when the server
      // moved it: the window is this event's.
      if (was && same(was[field], c) && same(was[field], s)) continue
      diff.sessions.push({ id, field, core: c ?? null, server: s ?? null })
    }
  }
  const coreLines = input.outcome.effects.flatMap((e) => (e.kind === `post` ? [coreKey(e)] : []))
  const [coreOnly, serverOnly] = multisetDiff(coreLines, input.posted.map(postedKey))
  diff.posts = { coreOnly, serverOnly }
  return diff
}

export const isMatch = (diff: ShadowDiff) =>
  !diff.refusal && diff.sessions.length === 0 && diff.posts.coreOnly.length === 0 && diff.posts.serverOnly.length === 0
