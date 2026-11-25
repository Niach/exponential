// #EXP-2138: deleting a chat, its agent or its team removes what a device
// holds of it. fixtures/held-gone.json is the spec; mirrors: the server's
// `sessions.heldGone` (the first half), desktop `domain::held_gone` (the rest).
import fixture from "../fixtures/held-gone.json" with { type: "json" }

export const HELD_GONE_BATCH = fixture.limits.batch
export const HELD_GONE_ASK_EVERY_HOURS = fixture.limits.askEveryHours
export const HELD_GONE_CONFIRM_HOURS = fixture.limits.confirmHours
export const HELD_GONE_CAP_COUNT = fixture.limits.capCount
/** `reason` of the kill a delete sends. */
export const KILL_REASON_DELETED = fixture.limits.killReason

export interface HeldRow {
  userId: string
  teamId: string
  shared: boolean
  deviceId: string
}

export interface HeldCaller {
  userId: string
  teamIds: readonly string[]
  /** The machines the caller has a devices row of. */
  deviceIds: readonly string[]
}

export type HeldAnswer = `kept` | `gone` | `unreadable`

/** What the caller is told about a session it holds something of. */
export function heldAnswer(row: HeldRow | null, caller: HeldCaller): HeldAnswer {
  if (!row) return `gone`
  const kept =
    row.userId === caller.userId ||
    (row.shared && caller.teamIds.includes(row.teamId)) ||
    (row.deviceId !== `` && caller.deviceIds.includes(row.deviceId))
  return kept ? `kept` : `unreadable`
}

export interface HeldRecord {
  id: string
  account: string
  aside: boolean
  cwd?: string
  scratch?: boolean
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The ids an account asks for. */
export function heldCandidates(account: string, records: readonly HeldRecord[], store: readonly string[]): string[] {
  const own = records.filter((record) => record.account === account && !record.aside).map((record) => record.id)
  return [...new Set([...own, ...store])].filter((id) => UUID.test(id)).sort()
}

/** The same answers of an id so far (ms). */
export interface GoneAnswers {
  kind: `gone` | `unreadable`
  first: number
  last: number
}

export interface HeldPassInput {
  now: number
  holds: readonly string[]
  /** The ids the chat store names. */
  cached: readonly string[]
  asked: readonly string[]
  /** null = the ask failed, was refused or did not parse. */
  answer: { gone: readonly string[]; unreadable: readonly string[] } | null
  state: Readonly<Record<string, GoneAnswers>>
  /** The ids a delete was announced for, before the ask. */
  notices: readonly string[]
}

export interface HeldPass {
  /** Deleted: everything goes. */
  remove: string[]
  /** Not readable any more: only the cached rows go. */
  uncache: string[]
  state: Record<string, GoneAnswers>
  notices: string[]
  capped: boolean
}

export function heldPass(input: HeldPassInput): HeldPass {
  if (input.answer === null) {
    return { remove: [], uncache: [], state: { ...input.state }, notices: [...input.notices], capped: false }
  }
  const holds = new Set(input.holds)
  const cached = new Set(input.cached)
  const unreadable = new Set(input.answer.unreadable)
  const gone = new Set(input.answer.gone)
  const noticed = new Set(input.notices)
  const state = Object.fromEntries(Object.entries(input.state).filter(([id]) => holds.has(id)))
  const notices = new Set(input.notices.filter((id) => holds.has(id)))
  const explicit: string[] = []
  const due = { gone: [] as string[], unreadable: [] as string[] }
  for (const id of input.asked) {
    if (!holds.has(id)) continue
    const kind = unreadable.has(id) ? `unreadable` : gone.has(id) ? `gone` : null
    if (kind !== `gone`) notices.delete(id)
    if (kind === null || (kind === `unreadable` && !cached.has(id))) {
      delete state[id]
      continue
    }
    if (kind === `gone` && noticed.has(id)) {
      explicit.push(id)
      continue
    }
    const held = state[id]
    const first = held && held.kind === kind && input.now >= held.last ? held.first : input.now
    state[id] = { kind, first, last: input.now }
    if (input.now - first >= HELD_GONE_CONFIRM_HOURS * 3_600_000) due[kind].push(id)
  }
  const count = due.gone.length + due.unreadable.length
  const capped = count > HELD_GONE_CAP_COUNT && count * 2 > input.holds.length
  return {
    remove: [...(capped ? [] : due.gone), ...explicit],
    uncache: capped ? [] : due.unreadable,
    state,
    notices: [...notices],
    capped,
  }
}

export interface HeldRemoval {
  record: boolean
  journal: boolean
  scratch: boolean
  store: boolean
}

/** What of `id` goes: `store` / `otherStore` = this / another account's chat
 *  store on the machine names it. */
export function heldRemoval(
  account: string,
  id: string,
  records: readonly HeldRecord[],
  store: boolean,
  otherStore: boolean
): HeldRemoval {
  const record = records.find((each) => each.id === id)
  const own = record !== undefined && record.account === account && !record.aside
  return {
    record: own,
    journal: own || (record === undefined && store && !otherStore),
    scratch: own && !!record.scratch && !records.some((each) => each.id !== id && each.cwd === record.cwd),
    store,
  }
}
