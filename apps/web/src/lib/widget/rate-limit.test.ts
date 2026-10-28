import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  clientIpFromRequest,
  proxyAttestedClientIp,
  rateLimitIpKey,
  resetWidgetRateLimitersForTest,
  takeWidgetConfirmationMailToken,
  takeWidgetIpToken,
  TokenBucketLimiter,
  WIDGET_SUBMIT_LIMITS,
} from "./rate-limit"

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

  // NEW-85: one IPv6 subscriber rotating interface ids inside its /64 is one
  // bucket, and a spoofed leftmost entry never moves the key.
  it(`keys IPv6 on the /64 of the attested hop`, () => {
    for (const hop of [`2001:db8:1:2::1`, `2001:db8:1:2:dead:beef:0:7`, `[2001:DB8:1:2::99]:443`]) {
      expect(clientIpFromRequest(withForwarded(`9.9.9.9, ${hop}`))).toBe(`2001:db8:1:2::/64`)
    }
    expect(clientIpFromRequest(withForwarded(`2001:db8:1:3::1`))).toBe(`2001:db8:1:3::/64`)
  })

  describe(`TRUSTED_PROXY_HOPS`, () => {
    afterEach(() => {
      vi.unstubAllEnvs()
    })

    it(`defaults to one appending proxy (the last hop)`, () => {
      expect(proxyAttestedClientIp(withForwarded(`1.1.1.1, 2.2.2.2, 3.3.3.3`))).toBe(`3.3.3.3`)
    })

    it(`takes the Nth hop from the right behind N proxies`, () => {
      vi.stubEnv(`TRUSTED_PROXY_HOPS`, `2`)
      expect(clientIpFromRequest(withForwarded(`1.1.1.1, 2.2.2.2, 3.3.3.3`))).toBe(`2.2.2.2`)
      // Fewer hops than proxies: the request skipped one; key on the leftmost.
      expect(clientIpFromRequest(withForwarded(`3.3.3.3`))).toBe(`3.3.3.3`)
    })

    it(`ignores a junk value`, () => {
      vi.stubEnv(`TRUSTED_PROXY_HOPS`, `zero`)
      expect(clientIpFromRequest(withForwarded(`1.1.1.1, 3.3.3.3`))).toBe(`3.3.3.3`)
    })

    it(`returns null without entries`, () => {
      expect(proxyAttestedClientIp(withForwarded(null))).toBeNull()
      expect(proxyAttestedClientIp(withForwarded(` , `))).toBeNull()
    })
  })
})

describe(`rateLimitIpKey (NEW-85)`, () => {
  it.each([
    // IPv4 keys on the address (a port is dropped).
    [`203.0.113.7`, `203.0.113.7`],
    [` 203.0.113.7:51234 `, `203.0.113.7`],
    // IPv6: the /64, whatever the spelling.
    [`2001:db8:aa:bb:1:2:3:4`, `2001:db8:aa:bb::/64`],
    [`2001:0DB8:00AA:00bb:0000:0000:0000:0001`, `2001:db8:aa:bb::/64`],
    [`2001:db8::1`, `2001:db8:0:0::/64`],
    [`2001:db8::`, `2001:db8:0:0::/64`],
    [`::1`, `0:0:0:0::/64`],
    [`::`, `0:0:0:0::/64`],
    [`fe80::1%eth0`, `fe80:0:0:0::/64`],
    [`[fe80::1%25en0]:8443`, `fe80:0:0:0::/64`],
    [`[2001:db8:1:2::5]`, `2001:db8:1:2::/64`],
    [`64:ff9b::192.0.2.1`, `64:ff9b:0:0::/64`],
    [`2001:db8:1:2:3:4:192.0.2.1`, `2001:db8:1:2::/64`],
    // IPv4-mapped = the IPv4 address (dual-stack sockets report these).
    [`::ffff:203.0.113.7`, `203.0.113.7`],
    [`::FFFF:cb00:7107`, `203.0.113.7`],
    [`[::ffff:203.0.113.7]:443`, `203.0.113.7`],
    // Not an address: its own text.
    [`unknown`, `unknown`],
    [`1:2:3`, `1:2:3`],
    [`1::2::3`, `1::2::3`],
    [`2001:db8:1:2:3:4:5:6:7`, `2001:db8:1:2:3:4:5:6:7`],
    [`999.1.1.1`, `999.1.1.1`],
    [``, `unknown`],
  ])(`%s -> %s`, (raw, key) => {
    expect(rateLimitIpKey(raw)).toBe(key)
  })
})

// NEW-19: the submit policy documented in rate-limit.ts, at its defaults.
describe(`widget submit policy`, () => {
  const envNames = Object.values(WIDGET_SUBMIT_LIMITS).flatMap((limit) => [
    limit.burstEnv,
    limit.hourlyEnv,
  ])

  const clearEnv = () => {
    for (const name of envNames) delete process.env[name]
    resetWidgetRateLimitersForTest()
  }
  beforeEach(clearEnv)
  afterEach(clearEnv)

  const now = 1_000_000

  it(`one IP files 20 in a row, the 21st is limited, then 2 a minute`, () => {
    for (let index = 0; index < 20; index += 1) {
      expect(takeWidgetIpToken(`1.1.1.1`, now).ok).toBe(true)
    }
    const denied = takeWidgetIpToken(`1.1.1.1`, now)
    expect(denied.ok).toBe(false)
    if (!denied.ok) {
      // 120/hour = one token every 30 s.
      expect(denied.retryAfterSeconds).toBeGreaterThan(0)
      expect(denied.retryAfterSeconds).toBeLessThanOrEqual(30)
    }
    // Another IP is unaffected.
    expect(takeWidgetIpToken(`2.2.2.2`, now).ok).toBe(true)
    // A minute later: two more.
    expect(takeWidgetIpToken(`1.1.1.1`, now + 60_000).ok).toBe(true)
    expect(takeWidgetIpToken(`1.1.1.1`, now + 60_000).ok).toBe(true)
    expect(takeWidgetIpToken(`1.1.1.1`, now + 60_000).ok).toBe(false)
  })

  it(`the confirmation mail allows a burst of 3 per address`, () => {
    expect(takeWidgetConfirmationMailToken(`t@x.io`, now)).toBe(true)
    expect(takeWidgetConfirmationMailToken(`t@x.io`, now)).toBe(true)
    expect(takeWidgetConfirmationMailToken(`t@x.io`, now)).toBe(true)
    expect(takeWidgetConfirmationMailToken(`t@x.io`, now)).toBe(false)
    // Keyed on the lowercased address.
    expect(takeWidgetConfirmationMailToken(` T@X.io`, now)).toBe(false)
    expect(takeWidgetConfirmationMailToken(`u@x.io`, now)).toBe(true)
  })

  it(`every bucket is env-tunable`, () => {
    process.env.WIDGET_RATE_LIMIT_IP_BURST = `2`
    resetWidgetRateLimitersForTest()
    expect(takeWidgetIpToken(`5`, now).ok).toBe(true)
    expect(takeWidgetIpToken(`5`, now).ok).toBe(true)
    expect(takeWidgetIpToken(`5`, now).ok).toBe(false)
  })
})
