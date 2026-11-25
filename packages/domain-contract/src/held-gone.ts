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

/** Whether the caller may keep what it holds of this session. */
export function heldByCaller(row: HeldRow | null, caller: HeldCaller): boolean {
  if (!row) return false
  if (row.userId === caller.userId) return true
  if (row.shared && caller.teamIds.includes(row.teamId)) return true
  return row.deviceId !== `` && caller.deviceIds.includes(row.deviceId)
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

/** The gone answers of an id so far (ms). */
export interface GoneAnswers {
  first: number
  last: number
}

export interface HeldPassInput {
  now: number
  holds: readonly string[]
  asked: readonly string[]
  /** null = the ask failed, was refused or did not parse. */
  gone: readonly string[] | null
  state: Readonly<Record<string, GoneAnswers>>
  /** The ids a delete was announced for, before the ask. */
  notices: readonly string[]
}

export interface HeldPass {
  remove: string[]
  state: Record<string, GoneAnswers>
  notices: string[]
  capped: boolean
}

export function heldPass(input: HeldPassInput): HeldPass {
  if (input.gone === null) {
    return { remove: [], state: { ...input.state }, notices: [...input.notices], capped: false }
  }
  const holds = new Set(input.holds)
  const gone = new Set(input.gone)
  const noticed = new Set(input.notices)
  const state = Object.fromEntries(Object.entries(input.state).filter(([id]) => holds.has(id)))
  const notices = new Set(input.notices.filter((id) => holds.has(id)))
  const explicit: string[] = []
  const due: string[] = []
  for (const id of input.asked) {
    if (!holds.has(id)) continue
    if (!gone.has(id)) {
      delete state[id]
      notices.delete(id)
      continue
    }
    if (noticed.has(id)) {
      explicit.push(id)
      continue
    }
    const held = state[id]
    state[id] = held && input.now >= held.last ? { first: held.first, last: input.now } : { first: input.now, last: input.now }
    if (input.now - state[id].first >= HELD_GONE_CONFIRM_HOURS * 3_600_000) due.push(id)
  }
  const capped = due.length > HELD_GONE_CAP_COUNT && due.length * 2 > input.holds.length
  return { remove: [...(capped ? [] : due), ...explicit], state, notices: [...notices], capped }
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
