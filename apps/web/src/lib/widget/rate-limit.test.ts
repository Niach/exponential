import { describe, expect, it } from "vitest"
import { clientIpFromRequest, TokenBucketLimiter } from "./rate-limit"

describe(`TokenBucketLimiter`, () => {
  it(`allows up to capacity as a burst, then rejects`, () => {
    const limiter = new TokenBucketLimiter({ capacity: 3, refillPerHour: 60 })
    const now = 1_000_000

    expect(limiter.tryTake(`k`, now).ok).toBe(true)
    expect(limiter.tryTake(`k`, now).ok).toBe(true)
    expect(limiter.tryTake(`k`, now).ok).toBe(true)
    const denied = limiter.tryTake(`k`, now)
    expect(denied.ok).toBe(false)
    if (!denied.ok) {
      // 60/hour = 1/minute: next token in ≤ 60s.
      expect(denied.retryAfterSeconds).toBeGreaterThan(0)
      expect(denied.retryAfterSeconds).toBeLessThanOrEqual(60)
    }
  })

  it(`refills at the sustained rate`, () => {
    const limiter = new TokenBucketLimiter({ capacity: 2, refillPerHour: 60 })
    const start = 0
    expect(limiter.tryTake(`k`, start).ok).toBe(true)
    expect(limiter.tryTake(`k`, start).ok).toBe(true)
    expect(limiter.tryTake(`k`, start).ok).toBe(false)

    // One minute later one token has refilled.
    expect(limiter.tryTake(`k`, start + 60_000).ok).toBe(true)
    expect(limiter.tryTake(`k`, start + 60_000).ok).toBe(false)
  })

  it(`never refills past capacity`, () => {
    const limiter = new TokenBucketLimiter({ capacity: 2, refillPerHour: 60 })
    expect(limiter.tryTake(`k`, 0).ok).toBe(true)

    // A day later: capacity is still the burst ceiling.
    const later = 24 * 3_600_000
    expect(limiter.tryTake(`k`, later).ok).toBe(true)
    expect(limiter.tryTake(`k`, later).ok).toBe(true)
    expect(limiter.tryTake(`k`, later).ok).toBe(false)
  })

  it(`isolates keys`, () => {
    const limiter = new TokenBucketLimiter({ capacity: 1, refillPerHour: 60 })
    expect(limiter.tryTake(`a`, 0).ok).toBe(true)
    expect(limiter.tryTake(`a`, 0).ok).toBe(false)
    expect(limiter.tryTake(`b`, 0).ok).toBe(true)
  })

  it(`evicts stale full buckets once past maxEntries`, () => {
    const limiter = new TokenBucketLimiter({
      capacity: 1,
      refillPerHour: 3600,
      maxEntries: 2,
    })
    expect(limiter.tryTake(`a`, 0).ok).toBe(true)
    expect(limiter.tryTake(`b`, 0).ok).toBe(true)
    // `a` and `b` are full again after a second (3600/h = 1/s); inserting a
    // third key triggers eviction and must still succeed.
    expect(limiter.tryTake(`c`, 5_000).ok).toBe(true)
  })
})

describe(`clientIpFromRequest`, () => {
  const withForwarded = (value: string | null) =>
    new Request(`http://localhost/api/widget/submit`, {
      headers: value == null ? {} : { "x-forwarded-for": value },
    })

  it(`returns the single hop the proxy appended`, () => {
    expect(clientIpFromRequest(withForwarded(`203.0.113.7`))).toBe(
      `203.0.113.7`
    )
  })

  it(`takes the RIGHTMOST hop — client-supplied leftmost entries are spoofable`, () => {
    expect(
      clientIpFromRequest(withForwarded(`1.2.3.4, 5.6.7.8, 203.0.113.7`))
    ).toBe(`203.0.113.7`)
  })

  it(`falls back to "unknown" without the header`, () => {
    expect(clientIpFromRequest(withForwarded(null))).toBe(`unknown`)
    expect(clientIpFromRequest(withForwarded(``))).toBe(`unknown`)
  })
})
