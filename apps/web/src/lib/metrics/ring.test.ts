import { describe, expect, it } from "vitest"
import { MinuteRing, minuteKey } from "./ring"

// Fixed base so bucket keys are deterministic: 2026-01-02T03:04:00.000Z.
const BASE_MS = Date.UTC(2026, 0, 2, 3, 4, 0)
const MINUTE = 60_000

describe(`minuteKey`, () => {
  it(`formats the UTC minute`, () => {
    expect(minuteKey(Math.floor(BASE_MS / MINUTE))).toBe(`2026-01-02T03:04`)
  })
})

describe(`MinuteRing`, () => {
  it(`accumulates recordings into the right minute bucket`, () => {
    const ring = new MinuteRing(60)
    ring.record(BASE_MS, 10)
    ring.record(BASE_MS + 1_000, 30)
    ring.record(BASE_MS + MINUTE, 5)

    const buckets = ring.buckets(BASE_MS + MINUTE)
    expect(buckets).toEqual([
      { minute: `2026-01-02T03:04`, count: 2, totalMs: 40, maxMs: 30 },
      { minute: `2026-01-02T03:05`, count: 1, totalMs: 5, maxMs: 5 },
    ])
  })

  it(`resets a slot when a newer minute wraps around onto it`, () => {
    const ring = new MinuteRing(3)
    ring.record(BASE_MS, 100)
    // 3 minutes later lands on the same slot (3 % 3 === 0).
    ring.record(BASE_MS + 3 * MINUTE, 7)

    const buckets = ring.buckets(BASE_MS + 3 * MINUTE)
    expect(buckets).toEqual([
      { minute: `2026-01-02T03:07`, count: 1, totalMs: 7, maxMs: 7 },
    ])
  })

  it(`skips stale slots instead of reporting them`, () => {
    const ring = new MinuteRing(3)
    ring.record(BASE_MS, 1)
    // Window slid far past the recording — nothing inside it anymore.
    expect(ring.buckets(BASE_MS + 10 * MINUTE)).toEqual([])
  })

  it(`totals only the trailing lastMinutes`, () => {
    const ring = new MinuteRing(60)
    ring.record(BASE_MS, 0)
    ring.record(BASE_MS + MINUTE, 0)
    ring.record(BASE_MS + 9 * MINUTE, 0)
    const now = BASE_MS + 9 * MINUTE
    expect(ring.total(now, 5)).toBe(1)
    expect(ring.total(now)).toBe(3)
  })

  it(`aggregates latency across the window`, () => {
    const ring = new MinuteRing(60)
    ring.record(BASE_MS, 10)
    ring.record(BASE_MS + MINUTE, 20)
    ring.record(BASE_MS + MINUTE, 60)
    const { count, avgMs, maxMs } = ring.latency(BASE_MS + 2 * MINUTE)
    expect(count).toBe(3)
    expect(avgMs).toBe(30)
    expect(maxMs).toBe(60)
  })

  it(`reports zero latency on an empty window`, () => {
    const ring = new MinuteRing(60)
    expect(ring.latency(BASE_MS)).toEqual({ count: 0, avgMs: 0, maxMs: 0 })
  })
})
