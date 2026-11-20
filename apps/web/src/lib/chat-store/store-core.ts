// #EDE-115 (chat-store.json S4): the web's chat store, the worker's half.
// One APPEND-ONLY LOG per chat (root) in OPFS, read into the crate's
// `ChatFile` (chat-store-wasm) and appended with exactly what each write
// returns. Platform-free: the worker hands in the file host (OPFS sync
// access handles under a Web Lock per file, so every tab's worker may use
// the same file) and the wasm module; the tests hand in memory files.
//
// A file starts with a GENERATION line (`{"h":"<16 hex>"}`) the crate skips;
// a rewrite (RETENTION, a log grown far past its records) writes a new one,
// so a tab holding the file cached reloads it whole instead of reading the
// tail as an append.

import { CHAT_STORE_HITS_MAX, compareHits, type ChatHit } from "@exp/domain-contract/chat-store"

export type SyncFile = {
  getSize(): number
  read(buffer: Uint8Array, options: { at: number }): number
  write(buffer: Uint8Array, options: { at: number }): number
  truncate(size: number): void
  flush(): void
}

/** Runs `body` with `name`'s file open exclusively (every tab's worker). */
export type FileHost = {
  withFile<T>(name: string, body: (file: SyncFile) => T): Promise<T>
  /** #EDE-99: the names of the files that exist (global search). */
  list?(): Promise<string[]>
}

/** The generated `ChatFile` (crates/chat-store-wasm). */
export type ChatFileWasm = {
  load(text: string): number
  putEvents(eventsJson: string): string
  putMessages(messagesJson: string): string
  completeTo(sessionId: string, deviceId: string): number | undefined
  meta(key: string): string | undefined
  setMeta(key: string, value: string): string
  records(): number
  lines(): number
  snapshot(): string
  applyRetention(nowMs: number, windowDays: number | undefined): number
  openTail(sessionId: string, deviceId: string, tail: number): string
  page(sessionId: string, deviceId: string, before: number, limit: number): string
  haveRuns(sessionId: string, deviceId: string, afterSeq: number | undefined): string
  answerChunk(sessionId: string, pageJson: string, maxBytes: number): string | undefined
  take(rootId: string, sessionId: string, deviceId: string, afterSeq: number | undefined, chunkJson: string, nowMs: number): string
  searchChat(rootId: string, query: string, limit: number): string
  free(): void
}

export type ChatStoreWasm = {
  ChatFile: new () => ChatFileWasm
  Bucket: new (nowMs: number) => { spend(nowMs: number, bytes: number): void; waitMs(nowMs: number): number; free(): void }
  refusalFrame(pageJson: string, code: string): string | undefined
  holders(installsJson: string, deviceId: string, selfEndpoint: string, userId: string, skipJson: string): string
  mayRun(power: string, idleMs: number): boolean
  retryMs(attempts: number): number
  catchUpConstants(): string
}

export type StoredEvent = {
  rootId: string
  sessionId: string
  deviceId: string
  seq: number
  storedAt: number
  event: unknown
}

export type StoredMessage = {
  id: string
  rootId: string
  sessionId: string
  authorKind: string
  body: string | null
  createdAt: number
}

export type OpenedTail = { events: { seq: number; event: unknown }[]; truncated: boolean }
export type StoredPage = { events: unknown[]; seqs: number[] }
export type TakeResult = { added: number; completeTo: number | null; next: number | null; end: boolean }

/** RETENTION on the web (chat-store.json `retentionDays.web`). */
export const WEB_RETENTION_DAYS = 30
/** Chats held decoded at once (a big chat is tens of MB in the wasm heap). */
const CACHED_CHATS = 4
const HEADER_BYTES = 25
const DAY_MS = 86_400_000

type Cached = { file: ChatFileWasm; size: number; generation: string; used: number }

const encoder = new TextEncoder()
const decoder = new TextDecoder()

function newGeneration(): string {
  const bytes = new Uint8Array(8)
  crypto.getRandomValues(bytes)
  return [...bytes].map((byte) => byte.toString(16).padStart(2, `0`)).join(``)
}

function header(generation: string): string {
  return `{"h":"${generation}"}\n`
}

function readRange(file: SyncFile, from: number, to: number): string {
  if (to <= from) return ``
  const buffer = new Uint8Array(to - from)
  file.read(buffer, { at: from })
  return decoder.decode(buffer)
}

function readGeneration(file: SyncFile, size: number): string {
  if (size < HEADER_BYTES) return ``
  const match = /^\{"h":"([0-9a-f]{16})"\}\n/.exec(readRange(file, 0, HEADER_BYTES))
  return match?.[1] ?? ``
}

const ROOT_ID = /^[A-Za-z0-9_-]{1,128}$/

export class ChatStoreCore {
  private cache = new Map<string, Cached>()
  private compacted = new Set<string>()
  private bucket: InstanceType<ChatStoreWasm[`Bucket`]> | undefined

  constructor(
    private readonly wasm: ChatStoreWasm,
    private readonly host: FileHost,
    private readonly now: () => number = Date.now,
    private readonly retentionDays: number = WEB_RETENTION_DAYS
  ) {}

  /** One chat's file, current, under its lock: `body` may write (its
   *  returned lines are appended). */
  private withChat<T>(rootId: string, body: (chat: ChatFileWasm) => { value: T; append?: string }): Promise<T> {
    if (!ROOT_ID.test(rootId)) return Promise.reject(new Error(`not a chat id`))
    return this.host.withFile(`${rootId}.log`, (file) => {
      const chat = this.current(rootId, file)
      const { value, append } = body(chat.file)
      if (append) this.append(chat, file, append)
      chat.used = this.now()
      return value
    })
  }

  private current(rootId: string, file: SyncFile): Cached {
    const size = file.getSize()
    const held = this.cache.get(rootId)
    const generation = readGeneration(file, size)
    if (held && held.generation === generation && generation !== `` && size >= held.size) {
      // Another tab appended: take only the tail.
      if (size > held.size) held.file.load(readRange(file, held.size, size))
      held.size = size
      return held
    }
    held?.file.free()
    this.cache.delete(rootId)
    const chat = new this.wasm.ChatFile()
    let cached: Cached
    if (generation === ``) {
      // A new (or unreadable) file: start it over with a generation line.
      if (size > 0) chat.load(readRange(file, 0, size))
      cached = { file: chat, size: 0, generation: ``, used: this.now() }
      this.rewrite(cached, file)
    } else {
      chat.load(readRange(file, HEADER_BYTES, size))
      cached = { file: chat, size, generation, used: this.now() }
    }
    this.cache.set(rootId, cached)
    this.evict()
    if (!this.compacted.has(rootId)) {
      this.compacted.add(rootId)
      const dropped = chat.applyRetention(this.now(), this.retentionDays)
      if (dropped > 0 || chat.lines() > chat.records() * 2 + 1000) this.rewrite(cached, file)
    }
    return cached
  }

  private rewrite(chat: Cached, file: SyncFile) {
    chat.generation = newGeneration()
    const bytes = encoder.encode(header(chat.generation) + chat.file.snapshot())
    file.truncate(0)
    file.write(bytes, { at: 0 })
    file.flush()
    chat.size = bytes.length
  }

  private append(chat: Cached, file: SyncFile, text: string) {
    const bytes = encoder.encode(text)
    file.write(bytes, { at: chat.size })
    file.flush()
    chat.size += bytes.length
  }

  private evict() {
    while (this.cache.size > CACHED_CHATS) {
      let oldest: [string, Cached] | undefined
      for (const entry of this.cache) if (!oldest || entry[1].used < oldest[1].used) oldest = entry
      if (!oldest) return
      oldest[1].file.free()
      this.cache.delete(oldest[0])
    }
  }

  // ── Writes ───────────────────────────────────────────────────────────────

  /** A feed's events (one session on one device), as received. */
  putEvents(rootId: string, events: StoredEvent[]): Promise<void> {
    if (events.length === 0) return Promise.resolve()
    return this.withChat(rootId, (chat) => ({ value: undefined, append: chat.putEvents(JSON.stringify(events)) }))
  }

  /** Synced messages, any chats. */
  async putMessages(messages: StoredMessage[]): Promise<void> {
    const byRoot = new Map<string, StoredMessage[]>()
    for (const message of messages) {
      const list = byRoot.get(message.rootId) ?? []
      list.push(message)
      byRoot.set(message.rootId, list)
    }
    for (const [rootId, list] of byRoot) {
      await this.withChat(rootId, (chat) => ({ value: undefined, append: chat.putMessages(JSON.stringify(list)) }))
    }
  }

  // ── Reads ────────────────────────────────────────────────────────────────

  /** OPEN: the session's stored events on the device it is on now, cut to
   *  the REPLAY TAIL. */
  openTail(rootId: string, sessionId: string, deviceId: string, tail: number): Promise<OpenedTail> {
    return this.withChat(rootId, (chat) => ({ value: JSON.parse(chat.openTail(sessionId, deviceId, tail)) as OpenedTail }))
  }

  page(rootId: string, sessionId: string, deviceId: string, before: number, limit: number): Promise<StoredPage> {
    return this.withChat(rootId, (chat) => ({ value: JSON.parse(chat.page(sessionId, deviceId, before, limit)) as StoredPage }))
  }

  searchChat(rootId: string, query: string, limit: number): Promise<unknown> {
    return this.withChat(rootId, (chat) => ({ value: JSON.parse(chat.searchChat(rootId, query, limit)) }))
  }

  private searchRun = 0

  /** #EDE-99 GLOBAL SEARCH (chat-store.json `search.allCases`) over the
   *  log per chat: each existing file of `rootIds` (null = every one)
   *  answers its newest hit and its count; newest chat first, at most
   *  `limit`. A chat not held decoded is read into a throwaway file (never
   *  evicting an open chat). A newer search ends an older one: null. */
  async searchAll(rootIds: string[] | null, query: string, limit: number): Promise<{ chats: { rootId: string; count: number; hit: ChatHit }[]; total: number } | null> {
    const run = ++this.searchRun
    const allowed = rootIds ? new Set(rootIds) : null
    const names = (await this.host.list?.()) ?? []
    const chats: { rootId: string; count: number; hit: ChatHit }[] = []
    for (const name of names) {
      if (run !== this.searchRun) return null
      if (!name.endsWith(`.log`)) continue
      const rootId = name.slice(0, -4)
      if (!ROOT_ID.test(rootId) || (allowed && !allowed.has(rootId))) continue
      const found = await this.host.withFile(name, (file) => {
        if (this.cache.has(rootId)) return JSON.parse(this.current(rootId, file).file.searchChat(rootId, query, 1)) as { hits: ChatHit[]; total: number }
        const size = file.getSize()
        const temp = new this.wasm.ChatFile()
        try {
          temp.load(readRange(file, readGeneration(file, size) === `` ? 0 : HEADER_BYTES, size))
          return JSON.parse(temp.searchChat(rootId, query, 1)) as { hits: ChatHit[]; total: number }
        } finally {
          temp.free()
        }
      })
      const hit = found.hits[0]
      if (hit) chats.push({ rootId, count: found.total, hit })
    }
    if (run !== this.searchRun) return null
    chats.sort((a, b) => compareHits(a.hit, b.hit))
    return { chats: chats.slice(0, Math.max(1, Math.min(limit, CHAT_STORE_HITS_MAX))), total: chats.length }
  }

  // ── Catch-up (chat-store.json `catchUp`) ─────────────────────────────────

  meta(rootId: string, key: string): Promise<string | null> {
    return this.withChat(rootId, (chat) => ({ value: chat.meta(key) ?? null }))
  }

  /** Several values of one file's meta (the catch-up's index). */
  metas(rootId: string, keys: string[]): Promise<(string | null)[]> {
    return this.withChat(rootId, (chat) => ({ value: keys.map((key) => chat.meta(key) ?? null) }))
  }

  setMeta(rootId: string, key: string, value: string): Promise<void> {
    return this.withChat(rootId, (chat) => ({ value: undefined, append: chat.setMeta(key, value) }))
  }

  /** The next ask of a walk: `afterSeq` (null = from the first) is the
   *  cursor; null cursor on the first ask = this store's `completeTo`. */
  ask(rootId: string, sessionId: string, deviceId: string, afterSeq: number | null | undefined): Promise<{ afterSeq: number | null; have: [number, number][]; minAt: number }> {
    return this.withChat(rootId, (chat) => {
      const cursor = afterSeq === undefined ? (chat.completeTo(sessionId, deviceId) ?? null) : afterSeq
      const have = JSON.parse(chat.haveRuns(sessionId, deviceId, cursor ?? undefined)) as [number, number][]
      return { value: { afterSeq: cursor, have, minAt: this.now() - this.retentionDays * DAY_MS } }
    })
  }

  /** The asker takes one `history_chunk`. */
  take(rootId: string, sessionId: string, deviceId: string, afterSeq: number | null, chunk: unknown): Promise<TakeResult> {
    return this.withChat(rootId, (chat) => {
      const out = JSON.parse(chat.take(rootId, sessionId, deviceId, afterSeq ?? undefined, JSON.stringify(chunk), this.now())) as TakeResult & { append: string }
      const { append, ...value } = out
      return { value, append }
    })
  }

  /** THE HOLDER: the chunk answering a device-naming page off this store. */
  answer(rootId: string, sessionId: string, page: unknown, maxBytes: number): Promise<string | null> {
    return this.withChat(rootId, (chat) => ({ value: chat.answerChunk(sessionId, JSON.stringify(page), maxBytes) ?? null }))
  }

  refusal(page: unknown, code: string): string | null {
    return this.wasm.refusalFrame(JSON.stringify(page), code) ?? null
  }

  holders(installs: unknown[], deviceId: string, selfEndpoint: string, userId: string, skip: string[]): { endpointId: string; runner: boolean }[] {
    return JSON.parse(this.wasm.holders(JSON.stringify(installs), deviceId, selfEndpoint, userId, JSON.stringify(skip)))
  }

  mayRun(power: string, idleMs: number): boolean {
    return this.wasm.mayRun(power, idleMs)
  }

  retryMs(attempts: number): number {
    return this.wasm.retryMs(attempts)
  }

  constants(): { pageMax: number; concurrency: number; askTimeoutMs: number; passIntervalMs: number; endpointBackoffMs: number } {
    return JSON.parse(this.wasm.catchUpConstants())
  }

  /** The byte bucket: ms until the next ask may go. */
  bucketWait(): number {
    this.bucket ??= new this.wasm.Bucket(this.now())
    return this.bucket.waitMs(this.now())
  }

  bucketSpend(bytes: number): void {
    this.bucket ??= new this.wasm.Bucket(this.now())
    this.bucket.spend(this.now(), bytes)
  }
}
