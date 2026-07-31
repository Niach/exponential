import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { dailyAnonymousIdFromHeaders } from "@/lib/conversion/anonymous"
import {
  extractAttributionParams,
  externalReferrer,
  shouldCaptureLanding,
  truncateAttributionInput,
} from "@/lib/conversion/attribution"

const UA = `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36`

function landingRequest(
  url = `https://app.exponential.at/`,
  headers: Record<string, string> = {}
): Request {
  return new Request(url, {
    headers: {
      accept: `text/html,application/xhtml+xml`,
      "user-agent": UA,
      ...headers,
    },
  })
}

describe(`extractAttributionParams`, () => {
  it(`reads ref and utm params`, () => {
    const params = extractAttributionParams(
      new URL(
        `https://x.test/?ref=lobsters&utm_source=hn&utm_medium=social&utm_campaign=launch`
      )
    )
    expect(params).toEqual({
      ref: `lobsters`,
      utmSource: `hn`,
      utmMedium: `social`,
      utmCampaign: `launch`,
    })
  })

  it(`omits absent/empty params and truncates long ones`, () => {
    const params = extractAttributionParams(
      new URL(`https://x.test/?ref=${`a`.repeat(500)}&utm_source=%20%20`)
    )
    expect(params.ref).toHaveLength(128)
    expect(params.utmSource).toBeUndefined()
    expect(params.utmMedium).toBeUndefined()
  })
})

describe(`externalReferrer`, () => {
  it(`keeps a cross-host referrer`, () => {
    const req = landingRequest(`https://app.exponential.at/`, {
      referer: `https://news.ycombinator.com/item?id=1`,
    })
    expect(externalReferrer(req)).toBe(`https://news.ycombinator.com/item?id=1`)
  })

  it(`drops a same-host referrer (internal navigation)`, () => {
    const req = landingRequest(`https://app.exponential.at/pricing`, {
      referer: `https://app.exponential.at/`,
    })
    expect(externalReferrer(req)).toBeUndefined()
  })

  it(`tolerates malformed referrers`, () => {
    const req = landingRequest(`https://app.exponential.at/`, {
      referer: `not a url`,
    })
    expect(externalReferrer(req)).toBeUndefined()
  })
})

describe(`shouldCaptureLanding`, () => {
  it(`accepts a plain document GET`, () => {
    expect(shouldCaptureLanding(landingRequest())).toBe(true)
  })

  it(`rejects non-GET`, () => {
    const req = new Request(`https://x.test/`, {
      method: `POST`,
      headers: { accept: `text/html`, "user-agent": UA },
    })
    expect(shouldCaptureLanding(req)).toBe(false)
  })

  it(`rejects non-document accepts`, () => {
    const req = new Request(`https://x.test/`, {
      headers: { accept: `application/json`, "user-agent": UA },
    })
    expect(shouldCaptureLanding(req)).toBe(false)
  })

  it(`rejects excluded paths and assets`, () => {
    for (const path of [
      `/api/shapes/issues`,
      `/widget/v1/loader.js`,
      `/support/tok123`,
      `/favicon.ico`,
      `/assets/app.css`,
    ]) {
      expect(
        shouldCaptureLanding(landingRequest(`https://x.test${path}`))
      ).toBe(false)
    }
  })

  it(`rejects signed-in browsers (session cookie present)`, () => {
    const req = landingRequest(`https://x.test/`, {
      cookie: `better-auth.session_token=abc`,
    })
    expect(shouldCaptureLanding(req)).toBe(false)
  })

  it(`rejects bot user agents and missing UA`, () => {
    expect(
      shouldCaptureLanding(
        landingRequest(`https://x.test/`, { "user-agent": `Googlebot/2.1` })
      )
    ).toBe(false)
    const noUa = new Request(`https://x.test/`, {
      headers: { accept: `text/html` },
    })
    expect(shouldCaptureLanding(noUa)).toBe(false)
  })
})

describe(`truncateAttributionInput`, () => {
  it(`nulls empties and truncates`, () => {
    const out = truncateAttributionInput({
      ref: ` lobsters `,
      utmSource: ``,
      referrer: `https://x.test/${`p`.repeat(500)}`,
    })
    expect(out.ref).toBe(`lobsters`)
    expect(out.utmSource).toBeNull()
    expect(out.utmMedium).toBeNull()
    expect(out.creemRef).toBeNull()
    expect(out.referrer).toHaveLength(256)
    expect(out.landingPath).toBeNull()
  })

  it(`passes creemRef through but DROPS an over-long one (never truncates)`, () => {
    expect(truncateAttributionInput({ creemRef: ` tok_abc ` }).creemRef).toBe(
      `tok_abc`
    )
    expect(
      truncateAttributionInput({ creemRef: `t`.repeat(513) }).creemRef
    ).toBeNull()
    expect(truncateAttributionInput({ creemRef: `  ` }).creemRef).toBeNull()
  })
})

describe(`dailyAnonymousIdFromHeaders`, () => {
  const originalSecret = process.env.BETTER_AUTH_SECRET
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret`
  })
  afterEach(() => {
    if (originalSecret === undefined) delete process.env.BETTER_AUTH_SECRET
    else process.env.BETTER_AUTH_SECRET = originalSecret
  })

  const headers = (ip: string, ua = UA) =>
    new Headers({ "x-forwarded-for": ip, "user-agent": ua })

  it(`is deterministic within a day and rotates across days`, () => {
    const day1 = new Date(`2026-07-30T10:00:00Z`)
    const later = new Date(`2026-07-30T23:59:00Z`)
    const day2 = new Date(`2026-07-31T00:01:00Z`)
    const a = dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), day1)
    expect(a).toHaveLength(32)
    expect(dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), later)).toBe(a)
    expect(dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), day2)).not.toBe(a)
  })

  it(`differs per ip and per user agent`, () => {
    const now = new Date(`2026-07-30T10:00:00Z`)
    const base = dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), now)
    expect(dailyAnonymousIdFromHeaders(headers(`5.6.7.8`), now)).not.toBe(base)
    expect(
      dailyAnonymousIdFromHeaders(headers(`1.2.3.4`, `other-ua`), now)
    ).not.toBe(base)
  })

  it(`uses the LAST x-forwarded-for hop (proxy-attested)`, () => {
    const now = new Date(`2026-07-30T10:00:00Z`)
    expect(dailyAnonymousIdFromHeaders(headers(`9.9.9.9, 1.2.3.4`), now)).toBe(
      dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), now)
    )
  })

  it(`returns null without a forwarded ip or without a secret`, () => {
    const now = new Date(`2026-07-30T10:00:00Z`)
    expect(
      dailyAnonymousIdFromHeaders(new Headers({ "user-agent": UA }), now)
    ).toBeNull()
    delete process.env.BETTER_AUTH_SECRET
    expect(dailyAnonymousIdFromHeaders(headers(`1.2.3.4`), now)).toBeNull()
  })
})
