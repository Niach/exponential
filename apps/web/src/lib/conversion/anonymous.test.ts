import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { dailyAnonymousIdFromHeaders } from "@/lib/conversion/anonymous"

// The visitor id keys on the same proxy-attested hop as the limiters
// (`proxyAttestedClientIp`), so TRUSTED_PROXY_HOPS moves it too.
describe(`dailyAnonymousIdFromHeaders behind TRUSTED_PROXY_HOPS`, () => {
  const saved = { secret: process.env.BETTER_AUTH_SECRET, hops: process.env.TRUSTED_PROXY_HOPS }
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret`
  })
  afterEach(() => {
    for (const [key, value] of [[`BETTER_AUTH_SECRET`, saved.secret], [`TRUSTED_PROXY_HOPS`, saved.hops]] as const) {
      if (value === undefined) delete process.env[key]
      else process.env[key] = value
    }
  })

  const at = new Date(`2026-07-30T10:00:00Z`)
  const id = (forwarded: string) =>
    dailyAnonymousIdFromHeaders(new Headers({ "x-forwarded-for": forwarded, "user-agent": `ua` }), at)

  it(`reads the second-to-last hop behind two proxies`, () => {
    process.env.TRUSTED_PROXY_HOPS = `2`
    expect(id(`6.6.6.6, 1.2.3.4, 10.0.0.1`)).toBe(id(`9.9.9.9, 1.2.3.4, 10.0.0.2`))
    expect(id(`6.6.6.6, 1.2.3.4, 10.0.0.1`)).not.toBe(id(`6.6.6.6, 5.6.7.8, 10.0.0.1`))
  })
})
