// EXP-1527: the web's connection analytics (fixtures/steer-enet.json
// `connStats`). One `steer.view` per viewer open of a session feed: `settled`
// once the first frame applied (or `settleAfterMs` passed without one) and
// `ended` when the view goes. Privacy-safe eCore envelopes: no content, no
// user, team, session or device id. Buffered and posted to
// `connStats.record` (<= batchMax) every flushMs and on page hide; it never
// blocks the view nor shows an error.

import {
  STEER_CONN_STATS,
  steerConnEventValid,
  type SteerConnData,
  type SteerConnEvent,
  type SteerConnPath,
} from "@exp/domain-contract/steer-enet"
import webPackage from "../../../package.json" with { type: "json" }
import type { FeedStats } from "./steer-feed"

export const WEB_APP_VERSION: string = webPackage.version

export type ConnStatsSink = (events: SteerConnEvent[]) => Promise<unknown>

const newId = () =>
  globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`

/** The batching buffer: drops what fails the contract's validation. */
export class ConnStatsBuffer {
  private events: SteerConnEvent[] = []
  private timer: ReturnType<typeof setTimeout> | undefined

  constructor(
    private readonly sink: ConnStatsSink,
    private readonly flushMs = STEER_CONN_STATS.flushMs,
    private readonly batchMax = STEER_CONN_STATS.batchMax
  ) {}

  record(event: SteerConnEvent): void {
    if (!steerConnEventValid(event)) return
    // A sink that is down never grows the buffer without bound.
    if (this.events.length >= this.batchMax * 4) this.events.shift()
    this.events.push(event)
    this.timer ??= setTimeout(() => {
      this.timer = undefined
      this.flush()
    }, this.flushMs)
  }

  /** Posts everything buffered, in batches; failures are dropped silently. */
  flush(): void {
    clearTimeout(this.timer)
    this.timer = undefined
    while (this.events.length > 0) {
      const batch = this.events.splice(0, this.batchMax)
      try {
        void this.sink(batch).catch(() => {})
      } catch {
        // Analytics never surface.
      }
    }
  }

  get size(): number {
    return this.events.length
  }
}

/** One viewer open of a session feed (the store's view), as a FeedStats. */
export class SteerViewStats implements FeedStats {
  private openedAt: number | null = null
  private path: SteerConnPath | undefined
  private firstFrameMs: number | undefined
  private settled = false
  private settleTimer: ReturnType<typeof setTimeout> | undefined
  private fallbacks = 0
  private reconnects = 0
  private lastError: string | undefined
  private failures: Record<string, number> = {}

  constructor(
    private readonly record: (event: SteerConnEvent) => void,
    private readonly now: () => number = Date.now,
    private readonly settleAfterMs = STEER_CONN_STATS.settleAfterMs
  ) {}

  /** The view opened (its first dial); later calls are no-ops. */
  start(): void {
    if (this.openedAt !== null) return
    this.openedAt = this.now()
    this.settleTimer = setTimeout(() => this.settle(), this.settleAfterMs)
  }

  applied(path: SteerConnPath, reset: boolean): void {
    if (this.openedAt === null) return
    if (this.path === undefined) {
      this.path = path
      this.firstFrameMs = Math.max(0, Math.round(this.now() - this.openedAt))
      this.settle()
    } else if (reset) {
      this.reconnects++
    }
  }

  fallback(): void {
    this.fallbacks++
  }

  failure(errorClass: string): void {
    const known = (STEER_CONN_STATS.errorClasses as readonly string[]).includes(errorClass) ? errorClass : `other`
    this.lastError = known
    this.failures[known] = (this.failures[known] ?? 0) + 1
  }

  /** The view went (dispose): the `ended` event. */
  end(): void {
    if (this.openedAt === null) return
    this.settle()
    const durationMs = Math.max(0, Math.round(this.now() - this.openedAt))
    this.emit({
      ...this.outcome(),
      phase: `ended`,
      durationMs,
      fallbacks: this.fallbacks,
      reconnects: this.reconnects,
      ...(Object.keys(this.failures).length > 0 ? { failures: { ...this.failures } } : {}),
    })
    this.openedAt = null
  }

  private settle() {
    clearTimeout(this.settleTimer)
    if (this.settled || this.openedAt === null) return
    this.settled = true
    this.emit({ ...this.outcome(), phase: `settled` })
  }

  private outcome(): Omit<SteerConnData, `phase`> {
    const base = { platform: `web`, version: WEB_APP_VERSION }
    if (this.path !== undefined) {
      return {
        ...base,
        outcome: `ok`,
        path: this.path,
        firstFrameMs: this.firstFrameMs,
        ...(this.lastError ? { errorClass: this.lastError } : {}),
      }
    }
    return { ...base, outcome: `failed`, errorClass: this.lastError ?? `other` }
  }

  private emit(data: SteerConnData) {
    this.record({ v: 1, id: newId(), kind: `steer.view`, at: Math.round(this.now()), source: `steer-viewer`, data })
  }
}

// ── This tab's buffer ────────────────────────────────────────────────────────

let shared: ConnStatsBuffer | undefined

/** The tab's buffer over `sink` (connStats.record), flushed on page hide. */
export function connStatsBuffer(sink: ConnStatsSink): ConnStatsBuffer {
  if (shared) return shared
  const buffer = new ConnStatsBuffer(sink)
  shared = buffer
  globalThis.addEventListener?.(`pagehide`, () => buffer.flush())
  globalThis.document?.addEventListener?.(`visibilitychange`, () => {
    if (globalThis.document?.visibilityState === `hidden`) buffer.flush()
  })
  return buffer
}
