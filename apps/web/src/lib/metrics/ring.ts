// Fixed-size per-minute ring buffer — the storage primitive behind the admin
// performance page's in-memory metrics (EXP-553). One slot per trailing
// minute; a slot is reset in place the first time a newer minute writes into
// it, so recording is O(1) with zero allocation and the structure never
// grows. Everything here is process-local and resets on restart by design.

export interface MinuteBucket {
  /** Epoch minute (Math.floor(ms / 60_000)) this slot currently holds; -1
   * when the slot has never been written. */
  epochMinute: number
  count: number
  totalMs: number
  maxMs: number
}

/** UTC minute key (`YYYY-MM-DDTHH:MM`) — the wire/UI identity of a bucket. */
export function minuteKey(epochMinute: number): string {
  return new Date(epochMinute * 60_000).toISOString().slice(0, 16)
}

export class MinuteRing {
  private readonly slots: MinuteBucket[]

  constructor(readonly minutes = 60) {
    this.slots = Array.from({ length: minutes }, () => ({
      epochMinute: -1,
      count: 0,
      totalMs: 0,
      maxMs: 0,
    }))
  }

  record(nowMs: number, durationMs = 0): void {
    const epochMinute = Math.floor(nowMs / 60_000)
    const slot = this.slots[epochMinute % this.minutes]
    if (slot.epochMinute !== epochMinute) {
      slot.epochMinute = epochMinute
      slot.count = 0
      slot.totalMs = 0
      slot.maxMs = 0
    }
    slot.count += 1
    slot.totalMs += durationMs
    if (durationMs > slot.maxMs) slot.maxMs = durationMs
  }

  /** Non-stale buckets inside the trailing window, oldest → newest. Minutes
   * with no recordings are skipped, not emitted as zeros — the consumer
   * gap-fills against `minuteKey`. */
  buckets(
    nowMs: number
  ): { minute: string; count: number; totalMs: number; maxMs: number }[] {
    const current = Math.floor(nowMs / 60_000)
    const out: {
      minute: string
      count: number
      totalMs: number
      maxMs: number
    }[] = []
    for (let i = this.minutes - 1; i >= 0; i--) {
      const epochMinute = current - i
      if (epochMinute < 0) continue
      const slot = this.slots[epochMinute % this.minutes]
      if (slot.epochMinute !== epochMinute) continue
      out.push({
        minute: minuteKey(epochMinute),
        count: slot.count,
        totalMs: slot.totalMs,
        maxMs: slot.maxMs,
      })
    }
    return out
  }

  /** Sum of `count` over the trailing `lastMinutes` (default: whole window). */
  total(nowMs: number, lastMinutes = this.minutes): number {
    const current = Math.floor(nowMs / 60_000)
    const span = Math.min(lastMinutes, this.minutes)
    let sum = 0
    for (let i = 0; i < span; i++) {
      const epochMinute = current - i
      if (epochMinute < 0) break
      const slot = this.slots[epochMinute % this.minutes]
      if (slot.epochMinute === epochMinute) sum += slot.count
    }
    return sum
  }

  /** Aggregate latency over the whole window. */
  latency(nowMs: number): { count: number; avgMs: number; maxMs: number } {
    let count = 0
    let totalMs = 0
    let maxMs = 0
    for (const bucket of this.buckets(nowMs)) {
      count += bucket.count
      totalMs += bucket.totalMs
      if (bucket.maxMs > maxMs) maxMs = bucket.maxMs
    }
    return { count, avgMs: count > 0 ? totalMs / count : 0, maxMs }
  }
}
