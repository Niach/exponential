// VAPP-272: the twin of the crate's `synced::SyncedRecordStore`: one account's
// outbox across its apps over a host's `OutboxLog` and transport, and the
// synced rows the host feeds in. `fixtures/outbox.json` `syncedCases` is the
// contract. The web stays online-only and does not run it; the twin keeps the
// fixture replayed in both languages.

import type { RecordRow } from "@exponential_app/eui/host"

import type { Pushed, SyncTransport } from "./instance"
import { outboxOutcome, outboxPendingLabel, outboxPrune, outboxRequest, outboxRows, outboxVerb, type OutboxCall, type OutboxEntry } from "./outbox"

/** Where an account's entries persist, oldest first. */
export interface OutboxLog {
  entries(): Promise<OutboxEntry[]>
  push(entry: OutboxEntry): Promise<void>
  ack(writeId: string, at: string): Promise<void>
  remove(writeIds: string[]): Promise<void>
}

export interface SyncedListener {
  changed?(): void
  /** A replay nobody awaits was refused: the failed-action toast. */
  refused?(entry: OutboxEntry, verb: string, pushed: Pushed): void
}

/** A host call's answer: its entry's first outcome, or queued (answer from the entry). */
export type Submitted = { done: unknown } | { refused: { message: string | null; invalid: boolean } } | { queued: true }

export class SyncedRecordStore {
  private flushing: Promise<void> | null = null
  private again = false
  private readonly awaited = new Set<string>()
  private readonly outcomes = new Map<string, Submitted>()
  private readonly synced = new Map<string, RecordRow[]>()

  private constructor(
    private readonly log: OutboxLog,
    private readonly transport: SyncTransport,
    private readonly now: () => string,
    private readonly listener: SyncedListener | null,
    private queue: OutboxEntry[],
  ) {}

  /** Reads the log once: a restart picks the queue up where it stopped. */
  static async open(log: OutboxLog, transport: SyncTransport, now: () => string, listener: SyncedListener | null = null): Promise<SyncedRecordStore> {
    return new SyncedRecordStore(log, transport, now, listener, await log.entries())
  }

  entries(): OutboxEntry[] {
    return [...this.queue]
  }

  pendingLabel(app: string): string | null {
    return outboxPendingLabel(this.queue, app)
  }

  rows(app: string): RecordRow[] {
    return this.synced.get(app) ?? []
  }

  visibleRows(app: string, me: string | null): RecordRow[] {
    const entries = this.queue.filter((e) => e.appId === app)
    return entries.length === 0 ? this.rows(app) : (outboxRows(this.rows(app) as never, entries, me) as unknown as RecordRow[])
  }

  /** Persist a call's entry; alone and online it replays now and answers
   *  its first outcome, otherwise queued with nothing sent. */
  async submit(entry: OutboxEntry, online: boolean): Promise<Submitted> {
    await this.log.push(entry)
    const awaits = online && !this.queue.some((e) => e.state === `pending`)
    if (awaits) this.awaited.add(entry.writeId)
    this.queue.push(entry)
    this.listener?.changed?.()
    if (!awaits) return { queued: true }
    await this.flush()
    if (this.flushing) await this.flushing
    this.awaited.delete(entry.writeId)
    const outcome = this.outcomes.get(entry.writeId)
    this.outcomes.delete(entry.writeId)
    return outcome ?? { queued: true }
  }

  /** One flusher at a time: a call while one runs asks for another pass. */
  async flush(): Promise<void> {
    if (this.flushing) {
      this.again = true
      return
    }
    this.flushing = this.drain().finally(() => {
      this.flushing = null
    })
    await this.flushing
  }

  private async drain(): Promise<void> {
    for (;;) {
      const entry = this.queue.find((e) => e.state === `pending`)
      const stop = entry ? await this.replay(entry) : true
      if (!stop) continue
      const pending = this.queue.some((e) => e.state === `pending`)
      const go = this.again && pending
      this.again = false
      if (!go) return
    }
  }

  private async replay(entry: OutboxEntry): Promise<boolean> {
    const { call, input } = outboxRequest(entry)
    const pushed = await this.send(entry.appId, call, input)
    const outcome = outboxOutcome(entry, pushed.result)
    let answer: Submitted = { queued: true }
    if (outcome === `done`) {
      const at = this.now()
      await this.log.ack(entry.writeId, at)
      this.queue = this.queue.map((e) => (e.writeId === entry.writeId ? { ...e, state: `acked`, ackedAt: at } : e))
      answer = { done: pushed.answer ?? null }
    } else if (outcome === `dropped` || outcome === `refused`) {
      await this.log.remove([entry.writeId])
      this.queue = this.queue.filter((e) => e.writeId !== entry.writeId)
      if (outcome === `refused`) answer = { refused: { message: pushed.message ?? null, invalid: pushed.invalid ?? false } }
    }
    if (this.awaited.delete(entry.writeId)) this.outcomes.set(entry.writeId, answer)
    else if (outcome === `refused`) this.listener?.refused?.(entry, outboxVerb(entry), pushed)
    if (outcome === `done`) await this.prune()
    this.listener?.changed?.()
    return outcome === `retry`
  }

  private send(app: string, call: OutboxCall, input: Record<string, unknown>): Promise<Pushed> {
    if (this.transport.send) return this.transport.send(app, call, input)
    return this.transport.push(app, call, input).then((result) => ({ result }))
  }

  /** Feed `app`'s synced rows in: acked entries they reflect leave. */
  async setRows(app: string, rows: RecordRow[]): Promise<void> {
    this.synced.set(app, rows)
    await this.prune()
  }

  /** Drop acked entries reflected or past the grace. An app whose rows were
   *  never fed in keeps its entries (an empty cache reads as every delete
   *  reflected). */
  async prune(): Promise<void> {
    const now = this.now()
    const gone: string[] = []
    for (const app of new Set(this.queue.filter((e) => e.state !== `pending`).map((e) => e.appId))) {
      const rows = this.synced.get(app)
      if (!rows) continue
      const entries = this.queue.filter((e) => e.appId === app)
      const kept = new Set(outboxPrune(entries, rows as never, now).map((e) => e.writeId))
      gone.push(...entries.filter((e) => !kept.has(e.writeId)).map((e) => e.writeId))
    }
    if (gone.length === 0) return
    await this.log.remove(gone)
    this.queue = this.queue.filter((e) => !gone.includes(e.writeId))
    this.listener?.changed?.()
  }
}
