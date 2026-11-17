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
