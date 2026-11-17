// #EDE-115 (chat-store.json S4): the main thread's side of the web's chat
// store. Kept tiny on purpose (it rides the main bundle): the store itself
// lives in a worker (chat-store.worker.ts, its own chunk) that loads the
// wasm at runtime, started lazily after sign-in. What feeds it:
//
// * every steer feed a session view receives (`recordFeed`: the join replay,
//   live events, the history pages), keyed by the device the session is on;
// * the synced messages a chat loads (`recordMessages`), opened with the
//   tree key (a body whose key is not here yet is stored without one and
//   stored again once the key arrives);
// * the eNet catch-up in the Web Lock tab (catch-up.ts).
//
// A chat opens from it (`openStoredTail`) before the live feed attaches.
// Signing out stops it; the files stay (no wipe yet: the sign-out question
// is open).

import { contract } from "@exp/domain-contract"
import { sessionCollection } from "@/lib/collections"
import { currentKeySource, subscribeKeyChange } from "@/lib/enet/current-key-holder"
import { openBodyCached } from "@/lib/enet/sealed-open"
import type { OpenedTail, StoredEvent, StoredMessage, StoredPage } from "./store-core"

/** How long a feed's events gather before they go to the worker. */
const FEED_FLUSH_MS = 400
/** A store not up within this long does not hold a chat's open back. */
const OPEN_TIMEOUT_MS = 1_500
/** Message ids remembered as stored (beyond it the memory starts over). */
const SEEN_MAX = 20_000

let account: string | undefined
let worker: Worker | undefined
let ready: Promise<boolean> | undefined
let requests = 0
const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>()

type SessionRow = { id: string; rootId: string | null; deviceId: string | null }

function sessionRow(sessionId: string): SessionRow | undefined {
  return sessionCollection.get(sessionId) as SessionRow | undefined
}

function send(op: string, args: unknown[]): Promise<unknown> {
  const target = worker
  if (!target) return Promise.reject(new Error(`chat store stopped`))
  const id = ++requests
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject })
    target.postMessage({ id, op, args })
  })
}

function ensure(): Promise<boolean> {
  if (!account || typeof Worker === `undefined`) return Promise.resolve(false)
  if (ready) return ready
  const user = account
  ready = (async () => {
    const created = new Worker(new URL(`./chat-store.worker.ts`, import.meta.url), { type: `module` })
    created.onmessage = (event: MessageEvent<{ id: number; result?: unknown; error?: string }>) => {
      const waiting = pending.get(event.data.id)
      if (!waiting) return
      pending.delete(event.data.id)
      if (event.data.error !== undefined) waiting.reject(new Error(event.data.error))
      else waiting.resolve(event.data.result)
    }
    worker = created
    return (await send(`init`, [user])) === true
  })().catch(() => false)
  return ready
}

// POWER (catchUp): on battery the catch-up runs only once the person is idle
// in EVERY tab, so each tab stamps its last input where the loop's tab reads.
const ACTIVE_KEY = `exp-chat-store-active`
const ACTIVE_EVERY_MS = 30_000
let activeAt = Date.now()
let activityBound = false

function noteActive() {
  const at = Date.now()
  if (at - activeAt < ACTIVE_EVERY_MS) return
  activeAt = at
  try {
    localStorage.setItem(ACTIVE_KEY, String(at))
  } catch {
    // storage full or blocked: this tab's own stamp still counts
  }
}

/** Ms of the newest input in any tab. */
export function lastActiveAt(): number {
  let shared = 0
  try {
    shared = Number(localStorage.getItem(ACTIVE_KEY)) || 0
  } catch {
    shared = 0
  }
  return Math.max(activeAt, shared)
}

/** Sign-in: this account's store (started once the tab is idle). */
export function startChatStore(userId: string): void {
  if (account === userId) return
  stopChatStore()
  account = userId
  if (!activityBound && typeof window !== `undefined`) {
    activityBound = true
    for (const type of [`pointerdown`, `keydown`, `wheel`]) window.addEventListener(type, noteActive, { passive: true, capture: true })
  }
  const warm = () => void ensure()
  if (typeof requestIdleCallback === `function`) requestIdleCallback(warm, { timeout: 5_000 })
  else setTimeout(warm, 1_000)
}

/** Sign-out (or unmount): nothing more is written; the files stay. */
export function stopChatStore(): void {
  worker?.terminate()
  worker = undefined
  ready = undefined
  account = undefined
  for (const waiting of pending.values()) waiting.reject(new Error(`chat store stopped`))
  pending.clear()
  feeds.clear()
  seen.clear()
  locked.clear()
}

/** One op on the worker; undefined = no store here (unsupported, stopped). */
export async function chatStoreCall<T>(op: string, ...args: unknown[]): Promise<T | undefined> {
  if (!(await ensure())) return undefined
  return (await send(op, args)) as T
}

// ── The steer feed ─────────────────────────────────────────────────────────

const feeds = new Map<string, { events: { seq: number; event: unknown }[]; timer: ReturnType<typeof setTimeout> }>()

/** Events a session view received from its feed (live, replay, pages). */
export function recordFeed(sessionId: string, events: { seq: number; event: unknown }[]): void {
  if (!account || events.length === 0) return
  let feed = feeds.get(sessionId)
  if (!feed) {
    feed = { events: [], timer: setTimeout(() => flushFeed(sessionId), FEED_FLUSH_MS) }
    feeds.set(sessionId, feed)
  }
  feed.events.push(...events)
}

function flushFeed(sessionId: string) {
  const feed = feeds.get(sessionId)
  feeds.delete(sessionId)
  const row = sessionRow(sessionId)
  if (!feed || !row?.deviceId) return
  const rootId = row.rootId ?? row.id
  const storedAt = Date.now()
  const events: StoredEvent[] = feed.events
    .filter(({ seq }) => Number.isSafeInteger(seq) && seq >= 0)
    .map(({ seq, event }) => ({ rootId, sessionId, deviceId: row.deviceId!, seq, storedAt, event }))
  void chatStoreCall(`putEvents`, rootId, events).catch(() => {})
}

/** Within `ms`, or undefined. */
function within<T>(promise: Promise<T>, ms: number): Promise<T | undefined> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(undefined), ms)
    promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      () => {
        clearTimeout(timer)
        resolve(undefined)
      }
    )
  })
}

/** OPEN: what the store holds of `sessionId` on the device it is on now,
 *  through the REPLAY TAIL; null = nothing (or no store here). */
export async function openStoredTail(sessionId: string): Promise<OpenedTail | null> {
  const row = sessionRow(sessionId)
  if (!account || !row?.deviceId) return null
  const tail = await within(
    chatStoreCall<OpenedTail>(`openTail`, row.rootId ?? row.id, sessionId, row.deviceId, contract.steerFeed.replayTail),
    OPEN_TIMEOUT_MS
  )
  return tail && tail.events.length > 0 ? tail : null
}

/** An older page off the store (the feed is not live). */
export async function storedPage(sessionId: string, before: number, limit: number): Promise<StoredPage | null> {
  const row = sessionRow(sessionId)
  if (!account || !row?.deviceId) return null
  return (await chatStoreCall<StoredPage>(`page`, row.rootId ?? row.id, sessionId, row.deviceId, before, limit).catch(() => undefined)) ?? null
}

// ── Synced messages ────────────────────────────────────────────────────────

/** The synced row fields the store keeps. */
export type MessageLike = {
  id: string
  sessionId: string | null
  rootId?: string | null
  authorKind?: string | null
  body: string
  createdAt: Date | string
}

/** id → what was stored (the raw body and whether it opened). */
const seen = new Map<string, string>()
/** Rows stored without a body: stored again when a key arrives. */
const locked = new Map<string, MessageLike>()
let keyWatch: (() => void) | undefined

function storedMessage(row: MessageLike): StoredMessage | null {
  if (!row.sessionId) return null
  const rootId = row.rootId || sessionRow(row.sessionId)?.rootId || row.sessionId
  const createdAt = new Date(row.createdAt).getTime()
  if (!Number.isFinite(createdAt)) return null
  const keys = currentKeySource()?.keysFor(rootId)
  const opened = keys ? openBodyCached(row.body, keys) : row.body.startsWith(`exp-sealed:`) ? null : { text: row.body }
  return { id: row.id, rootId, sessionId: row.sessionId, authorKind: row.authorKind ?? ``, body: opened?.text ?? null, createdAt }
}

/** The synced messages a chat loaded (any number of times: only what
 *  changed goes to the worker). */
export function recordMessages(rows: readonly MessageLike[]): void {
  if (!account || rows.length === 0) return
  const out: StoredMessage[] = []
  for (const row of rows) {
    const message = storedMessage(row)
    if (!message) continue
    const mark = `${message.body === null ? 0 : 1}:${row.body}`
    if (seen.get(row.id) === mark) continue
    if (seen.size >= SEEN_MAX) seen.clear()
    seen.set(row.id, mark)
    if (message.body === null) locked.set(row.id, row)
    else locked.delete(row.id)
    out.push(message)
  }
  if (locked.size > 0 && !keyWatch) {
    keyWatch = subscribeKeyChange(() => {
      const retry = [...locked.values()]
      if (retry.length === 0) return
      recordMessages(retry)
    })
  }
  if (out.length > 0) void chatStoreCall(`putMessages`, out).catch(() => {})
}
