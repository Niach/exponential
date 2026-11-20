// #EDE-129 (fixtures/find-in-chat.json, its `$comment` = the spec ×4): find
// in chat. Pure: the menu entry, the query, the stops in reading order, the
// counter, the steps, keeping the current stop while answers arrive, what a
// landing does, the status line and the keys. Mirrors: desktop
// `domain::find_in_chat` (the IDE and eDE), iOS + Android in #EDE-130.
import fixture from "../fixtures/find-in-chat.json" with { type: "json" }

export const FIND_IN_CHAT_COPY = fixture.copy
export const FIND_IN_CHAT_TEST_IDS = fixture.testIds
export const FIND_IN_CHAT_SHORTCUT = fixture.shortcut
export const FIND_IN_CHAT_HITS_MAX = fixture.hitsMax
export const FIND_IN_CHAT_DEBOUNCE_MS = fixture.debounceMs

const KINDS_WITH_TRANSCRIPT: readonly string[] = [`session`, `agent`, `assistant`]

/** The `…` menu with `search` first in its own group (an app: unchanged). */
export function findInChatMenu<E extends string>(kind: string, entries: readonly E[]): (E | `search` | `separator`)[] {
  if (!KINDS_WITH_TRANSCRIPT.includes(kind)) return [...entries]
  return entries.length > 0 ? [`search`, `separator`, ...entries] : [`search`]
}

/** The query as the chat store takes it: ASCII whitespace runs = one
 *  space, trimmed, the first `queryMaxChars` code points. */
export function findInChatQuery(raw: string): string {
  const collapsed = raw.replace(/[ \t\n\r\f\v]+/g, ` `).trim()
  return Array.from(collapsed).slice(0, fixture.queryMaxChars).join(``)
}

/** A loaded row, top to bottom. `count` = its painted highlight count. */
export interface FindRow {
  key: string
  count: number
  group?: string | null
  /** A tool row whose output matches: one more stop, ringing the row. */
  outputHit?: boolean
}

/** An older hit (store, then device), newest first. */
export interface FindOlderHit {
  key: string
  output?: boolean
}

/** #EXP-1998: a store hit as `findInChatOlder` reads it (an event's carries
 *  its `seq`, a synced message's its `messageId`). */
export interface FindStoreHit {
  sessionId: string
  seq?: number | null
  messageId?: string | null
  kind: string
}

/** #EXP-1998: the synced messages the host draws as bubbles (null = a live
 *  transcript, which holds them as its own rows): `loaded` = every loaded
 *  synced message of the session, `drawable` = those it would draw with no
 *  cap, `more` = older ones exist unloaded. */
export interface FindSynced {
  loaded: readonly string[]
  drawable: readonly string[]
  more: boolean
}

/** #EXP-1998 (`olderCases`): the store's and the device's hits that are
 *  OLDER than the loaded rows, store first, first key wins. */
export function findInChatOlder(input: {
  sessionId: string
  lowestSeq: number | null | undefined
  store: readonly FindStoreHit[]
  device: readonly { seq: number; kind: string }[]
  synced: FindSynced | null
}): FindOlderHit[] {
  const { sessionId, lowestSeq, synced } = input
  const loaded = new Set(synced?.loaded ?? [])
  const drawable = new Set(synced?.drawable ?? [])
  const seen = new Set<string>()
  const out: FindOlderHit[] = []
  const push = (key: string, output: boolean) => {
    if (seen.has(key)) return
    seen.add(key)
    out.push({ key, output })
  }
  const event = (seq: number, kind: string) => {
    if (lowestSeq === null || lowestSeq === undefined || seq < lowestSeq) push(findEventKey(sessionId, seq), kind === `tool_output`)
  }
  for (const hit of input.store) {
    if (hit.sessionId !== sessionId) continue
    if (typeof hit.seq === `number`) event(hit.seq, hit.kind)
    else if (hit.messageId && synced) {
      const id = hit.messageId
      if (loaded.has(id) ? drawable.has(id) : synced.more) push(findMessageKey(id), false)
    }
  }
  for (const hit of input.device) event(hit.seq, hit.kind)
  return out
}

/** One stop of the counter. */
export interface FindStop {
  key: string
  local: number
  loaded: boolean
  group: string | null
  output: boolean
}

/** Every stop in reading order (newest last). */
export function findInChatStops(rows: readonly FindRow[], older: readonly FindOlderHit[]): FindStop[] {
  const loaded = new Set(rows.map((r) => r.key))
  const seen = new Set<string>()
  const old: FindStop[] = []
  for (const hit of older) {
    if (loaded.has(hit.key) || seen.has(hit.key)) continue
    seen.add(hit.key)
    old.push({ key: hit.key, local: 0, loaded: false, group: null, output: hit.output === true })
  }
  old.reverse()
  const out = old
  for (const row of rows) {
    const group = row.group ?? null
    for (let local = 0; local < row.count; local++) out.push({ key: row.key, local, loaded: true, group, output: false })
    if (row.outputHit) out.push({ key: row.key, local: 0, loaded: true, group, output: true })
  }
  return out
}

/** The counter text (`""` = none shown). `current` is 0-based. */
export function findInChatCounter(input: { current: number | null; total: number; done: boolean; compact: boolean }): string {
  if (input.total === 0 || input.current === null) return input.done && input.total === 0 ? fixture.copy.noResults : ``
  const template = input.compact ? fixture.copy.counterCompact : fixture.copy.counter
  return template.replace(`{n}`, String(input.current + 1)).replace(`{m}`, String(input.total))
}

export type FindStep = `start` | `previous` | `next`

/** The stop after a step (wraps); `start` = the newest. */
export function findInChatStep(current: number | null, total: number, step: FindStep): number | null {
  if (total <= 0) return null
  if (step === `start` || current === null) return total - 1
  if (step === `previous`) return (current - 1 + total) % total
  return (current + 1) % total
}

/** The same stop in the new list (same key, local, output), else the newest. */
export function findInChatKeep(before: readonly FindStop[], current: number | null, after: readonly FindStop[]): number | null {
  if (after.length === 0) return null
  const was = current === null ? undefined : before[current]
  if (was) {
    const at = after.findIndex((s) => s.key === was.key && s.local === was.local && s.output === was.output)
    if (at >= 0) return at
  }
  return after.length - 1
}

/** What landing on a stop does. `current` = the row's `highlight.current`. */
export interface FindLanding {
  openGroup: string | null
  pageIn: boolean
  scroll: `centre`
  ring: boolean
  current: number | null
}

export function findInChatLand(stop: FindStop): FindLanding {
  if (!stop.loaded) return { openGroup: null, pageIn: true, scroll: `centre`, ring: true, current: null }
  return { openGroup: stop.group, pageIn: false, scroll: `centre`, ring: stop.output, current: stop.output ? null : stop.local }
}

export type FindSearchPhase = `none` | `searching` | `done` | `offline`

/** The status line under the bar (null = none). */
export function findInChatStatus(input: { store: FindSearchPhase; device: FindSearchPhase; deviceName: string }): string | null {
  if (input.device === `offline`) return fixture.copy.deviceOffline.replace(`{device}`, input.deviceName)
  if (input.device === `searching`) return fixture.copy.searchingDevice.replace(`{device}`, input.deviceName)
  if (input.store === `searching`) return fixture.copy.searchingOlder
  return null
}

export type FindKeyAction = `find` | `global` | `previous` | `next` | `close` | null

/** A key press: `mod` = Cmd on Apple, else Ctrl. */
export function findInChatKey(input: { key: string; mod: boolean; shift: boolean; inChat: boolean; inField: boolean }): FindKeyAction {
  const key = input.key.length === 1 ? input.key.toLowerCase() : input.key
  if (input.mod && key === `k`) return `global`
  if (input.mod && key === `f`) return input.inChat ? `find` : `global`
  if (!input.inField || input.mod) return null
  if (key === `Enter`) return input.shift ? `next` : `previous`
  if (key === `Escape`) return `close`
  return null
}

/** The tool row's hint beside its title (`n` = its output's matches). */
export function findInChatOutputHint(n: number): string {
  return n === 1 ? fixture.copy.outputHintOne : fixture.copy.outputHintMany.replace(`{n}`, String(n))
}

/** The menu's shortcut hint (phones show none). */
export function findInChatShortcut(apple: boolean): string {
  return apple ? fixture.shortcut.mac : fixture.shortcut.other
}

/** A hit's key: an event (seq = the tool row's for output) or a message. */
export function findEventKey(sessionId: string, seq: number): string {
  return `e:${sessionId}:${seq}`
}
export function findMessageKey(messageId: string): string {
  return `m:${messageId}`
}
