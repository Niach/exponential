// #EDE-105 THE CHAT STORE (fixtures/chat-store.json, whose `$comment` is the
// spec): the pure reference of what the store indexes, how it answers a
// search per chat and across chats, how it dedups what it is fed and what
// retention drops. The Rust crate `chat-store` (apps/desktop/crates/chat-store)
// replays the same cases over its in-memory core and its SQLite adapter.
import fixture from "../fixtures/chat-store.json" with { type: "json" }

export const CHAT_STORE_QUERY_MAX_CHARS = fixture.constants.queryMaxChars
export const CHAT_STORE_HITS_MAX = fixture.constants.hitsMax
export const CHAT_STORE_SNIPPET_CHARS = fixture.constants.snippetChars
export const CHAT_STORE_SNIPPET_LEAD_CHARS = fixture.constants.snippetLeadChars
export const CHAT_STORE_RETENTION_DAYS = fixture.constants.retentionDays

/** What a hit points at: the row kind a reader draws it in. */
export type ChatDocKind = `user_message` | `narration` | `tool` | `tool_output` | `message`

/** One transcript event as stored (the wire `activity` event + where it came from). */
export type StoredEvent = {
  rootId: string
  sessionId: string
  /** The device that ran it: a session moved in place restarts its seqs on the new device. */
  deviceId: string
  seq: number
  storedAt: number
  event: { kind: string } & Record<string, unknown>
}

/** One synced message as stored: `body` = the OPENED body, `null` while its chat key is missing. */
export type StoredMessage = {
  id: string
  rootId: string
  sessionId: string
  authorKind: string
  body: string | null
  createdAt: number
}

/** One searchable text. */
export type ChatDoc = {
  rootId: string
  sessionId: string
  deviceId?: string
  kind: ChatDocKind
  text: string
  at: number
  seq?: number
  toolId?: string
  messageId?: string
}

export type ChatHit = {
  rootId: string
  sessionId: string
  deviceId?: string
  kind: ChatDocKind
  at: number
  seq?: number
  messageId?: string
  snippet: string
  matchStart: number
  matchLen: number
  /** SYSTEM INPUTS (#FEED-300): present (true) on an input or message that is
   *  not a person's own words (`systemInput`). */
  system?: true
}

const SPACE = new Set([` `, `\t`, `\n`, `\r`, `\f`, `\v`])

/** Every run of ASCII whitespace = one space, trimmed (steer-enet.json SEARCH). */
export function collapse(text: string): string {
  let out = ``
  let space = false
  for (const ch of text) {
    if (SPACE.has(ch)) {
      space = out !== ``
      continue
    }
    if (space) out += ` `
    space = false
    out += ch
  }
  return out
}

/** The fold, per code point: NFD, marks dropped, lowercased; `source[i]` = the
 *  code point of `text` folded char `i` came from. */
export function fold(text: string): { chars: string[]; source: number[] } {
  const chars: string[] = []
  const source: number[] = []
  Array.from(text).forEach((ch, index) => {
    for (const part of ch.normalize(`NFD`)) {
      if (/\p{M}/u.test(part)) continue
      for (const lower of part.toLowerCase()) {
        chars.push(lower)
        source.push(index)
      }
    }
  })
  return { chars, source }
}

/** The folded query (collapsed, cut to `queryMaxChars`), empty = no search. */
export function foldQuery(query: string): string[] {
  return fold(Array.from(collapse(query)).slice(0, CHAT_STORE_QUERY_MAX_CHARS).join(``)).chars
}

/** The first match of `query` in `text` (collapsed first): snippet + offsets in code points. */
export function firstMatch(text: string, query: string[]): { snippet: string; matchStart: number; matchLen: number } | null {
  const collapsed = collapse(text)
  const { chars: folded, source } = fold(collapsed)
  if (query.length === 0 || folded.length < query.length) return null
  let at = -1
  outer: for (let i = 0; i + query.length <= folded.length; i++) {
    for (let j = 0; j < query.length; j++) if (folded[i + j] !== query[j]) continue outer
    at = i
    break
  }
  if (at < 0) return null
  const start = source[at]!
  const end = source[at + query.length - 1]! + 1
  const chars = Array.from(collapsed)
  const from = Math.max(0, start - CHAT_STORE_SNIPPET_LEAD_CHARS)
  const to = Math.min(chars.length, Math.max(from + CHAT_STORE_SNIPPET_CHARS, end))
  const lead = from > 0 ? `…` : ``
  const snippet = lead + chars.slice(from, to).join(``) + (to < chars.length ? `…` : ``)
  return { snippet, matchStart: start - from + lead.length, matchLen: end - start }
}

const SYSTEM_TAG = /^\[[a-z_]+\] /
const SYSTEM_SCHEDULE = /^Schedule `[^`\n]+`[^\n]* fired/
const SYSTEM_EVENT = /^Event `[^`\n]+` \(from /
const SYSTEM_SPEAKER = /^(?:Report|Message) from [^\n]+? \((?:session|member) [^,) \t\n\r\f\v]+(?:, ended)?\): /

/** SYSTEM INPUTS (#FEED-300, `search.systemCases`): is this input or message
 *  text a line the device or the server worded (a tagged system line, a
 *  firing, another speaker's line), not a person's own words? The lines
 *  transcript-grouping.json `feedInput` reads back. */
export function systemInput(text: string): boolean {
  return SYSTEM_TAG.test(text) || SYSTEM_SCHEDULE.test(text) || SYSTEM_EVENT.test(text) || SYSTEM_SPEAKER.test(text)
}

const str = (value: unknown) => (typeof value === `string` ? value : null)
const num = (value: unknown) => (typeof value === `number` ? value : null)

/** INDEX SCOPE: the docs one stored event yields (never thoughts). */
export function eventDocs(stored: StoredEvent): ChatDoc[] {
  const { event } = stored
  const base = { rootId: stored.rootId, sessionId: stored.sessionId, deviceId: stored.deviceId, seq: stored.seq, at: num(event.at) ?? stored.storedAt }
  switch (event.kind) {
    case `user_message`:
    case `narration`: {
      const text = str(event.text)
      return text ? [{ ...base, kind: event.kind, text }] : []
    }
    case `tool`: {
      const name = str(event.name) ?? ``
      const detail = str(event.detail)
      const text = detail ? `${name} ${detail}` : name
      return text ? [{ ...base, kind: `tool`, text }] : []
    }
    case `tool_update`: {
      const text = str(event.output) || str(event.diff)
      const toolId = str(event.id)
      return text && toolId ? [{ ...base, kind: `tool_output`, text, toolId }] : []
    }
    default:
      return []
  }
}

/** INDEX SCOPE: a synced message's doc (an unopened or empty body = none). */
export function messageDocs(message: StoredMessage): ChatDoc[] {
  if (!message.body) return []
  return [
    {
      rootId: message.rootId,
      sessionId: message.sessionId,
      kind: `message`,
      text: message.body,
      at: message.createdAt,
      messageId: message.id,
    },
  ]
}

/** The store's contents (what `ingest` built). */
export type ChatStoreState = { events: StoredEvent[]; messages: StoredMessage[] }

/** DEDUP: events by (sessionId, deviceId, seq), first write wins; messages by id,
 *  latest wins. Events are kept per session and device in seq order. */
export function ingest(
  state: ChatStoreState,
  batch: { events?: StoredEvent[]; messages?: StoredMessage[] }
): ChatStoreState {
  const key = (e: StoredEvent) => `${e.sessionId}\u0000${e.deviceId}\u0000${e.seq}`
  const seen = new Set(state.events.map(key))
  const events = [...state.events]
  for (const event of batch.events ?? []) {
    if (seen.has(key(event))) continue
    seen.add(key(event))
    events.push(event)
  }
  events.sort((a, b) => cmp(a.sessionId, b.sessionId) || cmp(a.deviceId, b.deviceId) || a.seq - b.seq)
  const messages = new Map(state.messages.map((m) => [m.id, m]))
  for (const message of batch.messages ?? []) messages.set(message.id, message)
  return { events, messages: [...messages.values()].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)) }
}

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

/** ORDER: newest first by `at`, then session id, then device id, then seq (higher first; a message
 *  after the session's events at the same instant), then message id. */
export function compareHits(a: ChatHit, b: ChatHit): number {
  if (a.at !== b.at) return b.at - a.at
  if (a.sessionId !== b.sessionId) return a.sessionId < b.sessionId ? -1 : 1
  const da = a.deviceId ?? ``
  const db = b.deviceId ?? ``
  if (da !== db) return da < db ? -1 : 1
  const sa = a.seq ?? -1
  const sb = b.seq ?? -1
  if (sa !== sb) return sb - sa
  const ma = a.messageId ?? ``
  const mb = b.messageId ?? ``
  return ma < mb ? -1 : ma > mb ? 1 : 0
}

/** Every hit of `query` in `state`, ordered, deduped (`rootId` = one chat). */
function allHits(state: ChatStoreState, query: string[], rootId?: string): ChatHit[] {
  const toolRows = new Map<string, number>()
  for (const e of state.events) {
    const id = str(e.event.id)
    if (e.event.kind !== `tool` || !id) continue
    const key = `${e.sessionId}\u0000${e.deviceId}\u0000${id}`
    const row = toolRows.get(key)
    if (row === undefined || e.seq < row) toolRows.set(key, e.seq)
  }
  const docs = [...state.events.flatMap(eventDocs), ...state.messages.flatMap(messageDocs)].filter(
    (doc) => rootId === undefined || doc.rootId === rootId
  )
  const hits: ChatHit[] = []
  const eventTexts = new Set<string>()
  const messageHits: { hit: ChatHit; text: string }[] = []
  for (const doc of docs) {
    const match = firstMatch(doc.text, query)
    if (!match) continue
    const seq = doc.toolId !== undefined ? (toolRows.get(`${doc.sessionId}\u0000${doc.deviceId}\u0000${doc.toolId}`) ?? doc.seq) : doc.seq
    const hit: ChatHit = {
      rootId: doc.rootId,
      sessionId: doc.sessionId,
      ...(doc.deviceId !== undefined ? { deviceId: doc.deviceId } : {}),
      kind: doc.kind,
      at: doc.at,
      ...(seq !== undefined ? { seq } : {}),
      ...(doc.messageId !== undefined ? { messageId: doc.messageId } : {}),
      ...match,
      ...((doc.kind === `user_message` || doc.kind === `message`) && systemInput(doc.text) ? { system: true as const } : {}),
    }
    if (doc.kind === `message`) {
      messageHits.push({ hit, text: collapse(doc.text) })
    } else {
      if (doc.kind === `narration` || doc.kind === `user_message`) eventTexts.add(`${doc.sessionId}\u0000${collapse(doc.text)}`)
      hits.push(hit)
    }
  }
  for (const { hit, text } of messageHits) {
    if (!eventTexts.has(`${hit.sessionId}\u0000${text}`)) hits.push(hit)
  }
  return hits.sort(compareHits)
}

const clampLimit = (limit: number) => Math.min(Math.max(limit, 1), CHAT_STORE_HITS_MAX)

/** FIND IN CHAT: one chat's hits, newest first, at most `limit`; `total` = all of them. */
export function searchChat(state: ChatStoreState, rootId: string, query: string, limit: number): { hits: ChatHit[]; total: number } {
  const folded = foldQuery(query)
  if (folded.length === 0) return { hits: [], total: 0 }
  const hits = allHits(state, folded, rootId)
  return { hits: hits.slice(0, clampLimit(limit)), total: hits.length }
}

/** GLOBAL SEARCH: one row per chat (its newest hit + how many), newest chat first, at most `limit`. */
export function searchAll(
  state: ChatStoreState,
  query: string,
  limit: number
): { chats: { rootId: string; count: number; hit: ChatHit }[]; total: number } {
  const folded = foldQuery(query)
  if (folded.length === 0) return { chats: [], total: 0 }
  const chats = new Map<string, { rootId: string; count: number; hit: ChatHit }>()
  for (const hit of allHits(state, folded)) {
    const chat = chats.get(hit.rootId)
    if (chat) chat.count++
    else chats.set(hit.rootId, { rootId: hit.rootId, count: 1, hit })
  }
  const rows = [...chats.values()]
  return { chats: rows.slice(0, clampLimit(limit)), total: rows.length }
}

/** Prose (inputs and narration) is a chat's messages: retention never drops it. */
const PROSE = new Set([`user_message`, `narration`])

/** RETENTION: the events kept (`windowDays` null = all): prose always, every other
 *  event (tool steps, their output, slots) only inside the window. Synced messages
 *  are always kept. */
export function retain(events: StoredEvent[], now: number, windowDays: number | null): StoredEvent[] {
  if (windowDays === null) return events
  const cutoff = now - windowDays * 86_400_000
  return events.filter((e) => PROSE.has(e.event.kind) || (num(e.event.at) ?? e.storedAt) >= cutoff)
}

// ---------------------------------------------------------------------------
// #EDE-108 S2 BACKGROUND CATCH-UP (`catchUp` in the fixture): device to device
// over eNet, a reader walks every chat's events it lacks from the runner or
// another reader device. These are the pure rules every client replays.
// ---------------------------------------------------------------------------

export const CHAT_STORE_CATCH_UP = fixture.catchUp.constants

/** A run of consecutive seqs, both ends included. */
export type SeqRun = [number, number]

/** `have`: the runs of `seqs` above `afterSeq` (all when null), the lowest `max`. */
export function haveRuns(seqs: number[], afterSeq: number | null, max: number): SeqRun[] {
  const runs: SeqRun[] = []
  for (const seq of [...new Set(seqs)].sort((a, b) => a - b)) {
    if (afterSeq !== null && seq <= afterSeq) continue
    const last = runs[runs.length - 1]
    if (last && last[1] === seq - 1) last[1] = seq
    else {
      if (runs.length === max) break
      runs.push([seq, seq])
    }
  }
  return runs
}

/** One held event as the answer reads it. */
export type HeldEvent = { seq: number; kind: string; at: number }

/** A device-naming `history_page`'s fields that shape the answer. */
export type CatchUpAsk = { afterSeq: number | null; limit: number; have: SeqRun[]; minAt?: number | null }

export type CatchUpAnswer = {
  seqs: number[]
  throughSeq: number | null
  done: boolean
  lastSeq: number | null
  fullTo: number | null
}

/** THE HOLDER's page: the first `limit` events above `afterSeq` the asker lacks. */
export function catchUpAnswer(events: HeldEvent[], fullTo: number | null, ask: CatchUpAsk): CatchUpAnswer {
  const limit = Math.max(1, Math.min(CHAT_STORE_CATCH_UP.pageMax, ask.limit))
  const sorted = [...events].sort((a, b) => a.seq - b.seq)
  const lastSeq = sorted.length ? sorted[sorted.length - 1].seq : null
  const minAt = ask.minAt ?? null
  const seqs: number[] = []
  for (const event of sorted) {
    if (ask.afterSeq !== null && event.seq <= ask.afterSeq) continue
    if (ask.have.some(([from, to]) => from <= event.seq && event.seq <= to)) continue
    if (minAt !== null && !PROSE.has(event.kind) && event.at < minAt) continue
    seqs.push(event.seq)
    if (seqs.length === limit) break
  }
  const full = seqs.length === limit
  const ends = [ask.afterSeq, lastSeq].filter((seq): seq is number => seq !== null)
  const throughSeq = full ? seqs[seqs.length - 1] : ends.length ? Math.max(...ends) : null
  const done = lastSeq === null || (throughSeq !== null && throughSeq >= lastSeq)
  return { seqs, throughSeq, done, lastSeq, fullTo }
}

const below = (a: number | null, b: number | null) => (a ?? -1) < (b ?? -1)

/** THE ASKER's PROGRESS after one answer. `nextAfter` null = the walk of this holder ends. */
export function catchUpProgress(
  completeTo: number | null,
  afterSeq: number | null,
  answer: Pick<CatchUpAnswer, `throughSeq` | `done` | `fullTo`>
): { completeTo: number | null; nextAfter: number | null } {
  const reach = answer.throughSeq === null || answer.fullTo === null ? null : Math.min(answer.throughSeq, answer.fullTo)
  let next = completeTo
  if (!below(completeTo, afterSeq) && reach !== null && below(completeTo, reach)) next = reach
  return { completeTo: next, nextAfter: answer.done ? null : answer.throughSeq }
}

export type WalkHolder = { name: string; runner?: boolean; reachable?: boolean; seqs: number[]; fullTo: number | null }

/** WALK one session over its holders in order (the fixture's `walkCases`). */
export function catchUpWalk(
  asker: { seqs: number[]; completeTo: number | null },
  holders: WalkHolder[],
  limit: number
): { seqs: number[]; completeTo: number | null; asks: Record<string, number>; caughtUp: boolean } {
  const seqs = new Set(asker.seqs)
  let completeTo = asker.completeTo
  const asks: Record<string, number> = {}
  for (const holder of holders) {
    if (holder.reachable === false) continue
    const events = holder.seqs.map((seq) => ({ seq, kind: `narration`, at: 0 }))
    let cursor = completeTo
    let answer: CatchUpAnswer
    for (;;) {
      answer = catchUpAnswer(events, holder.fullTo, { afterSeq: cursor, limit, have: haveRuns([...seqs], cursor, CHAT_STORE_CATCH_UP.haveRunsMax) })
      asks[holder.name] = (asks[holder.name] ?? 0) + 1
      for (const seq of answer.seqs) seqs.add(seq)
      const step = catchUpProgress(completeTo, cursor, answer)
      completeTo = step.completeTo
      if (step.nextAfter === null) break
      cursor = step.nextAfter
    }
    if (holder.runner && (answer.lastSeq === null || !below(completeTo, answer.lastSeq))) {
      return { seqs: [...seqs].sort((a, b) => a - b), completeTo, asks, caughtUp: true }
    }
  }
  return { seqs: [...seqs].sort((a, b) => a - b), completeTo, asks, caughtUp: false }
}

export type CatchUpInstall = { endpointId: string; userId: string; deviceId: string | null; lastSeenAt: string | null }

/** HOLDERS: the runner's installs, then the asker's own other installs, newest seen first. */
export function catchUpHolders(
  installs: CatchUpInstall[],
  deviceId: string,
  selfEndpoint: string,
  userId: string,
  skip: string[]
): { endpointId: string; runner: boolean }[] {
  const newest = (a: CatchUpInstall, b: CatchUpInstall) => cmp(b.lastSeenAt ?? ``, a.lastSeenAt ?? ``)
  const askable = installs.filter((i) => i.endpointId !== selfEndpoint && !skip.includes(i.endpointId))
  const runner = askable.filter((i) => i.deviceId === deviceId).sort(newest)
  const own = askable.filter((i) => i.deviceId !== deviceId && i.userId === userId).sort(newest)
  return [...runner.map((i) => ({ endpointId: i.endpointId, runner: true })), ...own.map((i) => ({ endpointId: i.endpointId, runner: false }))]
}

/** POWER: on battery only once idle; on mains or unknown always. */
export function catchUpMayRun(power: `ac` | `battery` | `unknown`, idleMs: number): boolean {
  return power !== `battery` || idleMs >= CHAT_STORE_CATCH_UP.idleAfterMs
}

/** A session served by readers alone is tried again after this long. */
export function catchUpRetryMs(attempts: number): number {
  return Math.min(CHAT_STORE_CATCH_UP.retryMaxMs, CHAT_STORE_CATCH_UP.passIntervalMs * 2 ** attempts)
}

/** The TOKEN BUCKET: `spend` an answer's bytes, `wait` = ms until the balance is back to zero. */
export function catchUpBucket(ops: ({ at: number; spend: number } | { at: number; wait: true })[]): number[] {
  const { bytesPerSecond: rate, burstBytes: cap } = CHAT_STORE_CATCH_UP
  let tokens = cap
  let at = 0
  const waits: number[] = []
  for (const op of ops) {
    tokens = Math.min(cap, tokens + Math.floor(((op.at - at) * rate) / 1000))
    at = op.at
    if (`spend` in op) tokens -= op.spend
    else waits.push(tokens >= 0 ? 0 : Math.ceil((-tokens * 1000) / rate))
  }
  return waits
}

// ---------------------------------------------------------------------------
// #EDE-120 SIGN-OUT (`signOut` in the fixture): only a deliberate sign-out
// deletes the account's store files (the store holds chats decrypted), after
// stopping the store. An expiry / lost token or an account switch keeps them.
// ---------------------------------------------------------------------------

/** Why the account is left. */
export type ChatStoreLeaveReason = `sign_out` | `expired` | `switch`

/** An account id the store may name a path by: one segment of 1 to 255
 *  `[A-Za-z0-9._-]`, never all dots (real desktop ids are
 *  `<host>--<userId>`, e.g. `app.exponential.at--8Jk3…`). */
const SAFE_ACCOUNT = /^(?!\.+$)[A-Za-z0-9._-]{1,255}$/

/** Whether `account` is one safe path segment (the SIGN-OUT rule). */
export function chatStoreSafeAccount(account: string): boolean {
  return SAFE_ACCOUNT.test(account)
}

/** The desktop store's own file names (beside the account's sync store). */
const DESKTOP_STORE_FILES = new Set([`chat-store.sqlite`, `chat-store.sqlite-wal`, `chat-store.sqlite-shm`, `chat-store.sqlite-journal`])

/** Whether `path` (relative to the data dir or the OPFS root) is one of
 *  `account`'s chat store files. */
export function isChatStoreFile(account: string, path: string): boolean {
  if (!SAFE_ACCOUNT.test(account)) return false
  const parts = path.split(`/`)
  if (parts.length !== 3 || parts[1] !== account || parts[2] === ``) return false
  if (parts[0] === `accounts`) return DESKTOP_STORE_FILES.has(parts[2]!)
  return parts[0] === `chat-store`
}

/** SIGN-OUT: the files leaving `account` deletes, given the files there are. */
export function chatStoreOnLeave(reason: ChatStoreLeaveReason, account: string, files: string[]): { delete: string[] } {
  return { delete: reason === `sign_out` ? files.filter((path) => isChatStoreFile(account, path)) : [] }
}
