import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { dailyAnonymousIdFromHeaders } from "@/lib/conversion/anonymous"
import {
  extractAttributionParams,
  externalReferrer,
  shouldCaptureLanding,
  shouldCaptureReturnVisit,
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
  it(`accepts document GETs on the entry allowlist`, () => {
    for (const path of [`/`, `/auth/login`, `/auth/signup`]) {
      expect(shouldCaptureLanding(landingRequest(`https://x.test${path}`))).toBe(
        true
      )
    }
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

  it(`rejects everything off the entry allowlist (EXP-522)`, () => {
    for (const path of [
      `/t/feedback/boards/exponential`,
      `/w/feedback/projects/exponential`,
      `/wp-json/`,
      `/about`,
      `/onboarding`,
      `/api/shapes/issues`,
      `/widget/v1/demo`,
      `/support/tok123`,
      `/invite/${`ab`.repeat(32)}`,
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

  it(`rejects token-credentialed requests (EXP-522)`, () => {
    expect(
      shouldCaptureLanding(
        landingRequest(`https://x.test/`, { authorization: `Bearer tok` })
      )
    ).toBe(false)
    expect(
      shouldCaptureLanding(
        landingRequest(`https://x.test/`, { "x-api-key": `expu_abc` })
      )
    ).toBe(false)
  })

  it(`rejects speculative prefetch/prerender navigations (EXP-522)`, () => {
    expect(
      shouldCaptureLanding(
        landingRequest(`https://x.test/`, {
          "sec-purpose": `prefetch;anonymous-client-ip`,
        })
      )
    ).toBe(false)
    expect(
      shouldCaptureLanding(
        landingRequest(`https://x.test/`, { purpose: `prefetch` })
      )
    ).toBe(false)
  })

  it(`rejects bot and unfurler user agents and missing UA`, () => {
    for (const ua of [
      `Googlebot/2.1`,
      `WhatsApp/2.23.20.0`,
      `facebookexternalhit/1.1`,
    ]) {
      expect(
        shouldCaptureLanding(landingRequest(`https://x.test/`, { "user-agent": ua }))
      ).toBe(false)
    }
    const noUa = new Request(`https://x.test/`, {
      headers: { accept: `text/html` },
    })
    expect(shouldCaptureLanding(noUa)).toBe(false)
  })
})

describe(`shouldCaptureReturnVisit`, () => {
  const cookie = { cookie: `better-auth.session_token=abc` }

  it(`accepts a signed-in document GET on any app path`, () => {
    for (const path of [`/`, `/t/feedback/boards/exponential`, `/account`]) {
      expect(
        shouldCaptureReturnVisit(landingRequest(`https://x.test${path}`, cookie))
      ).toBe(true)
    }
  })

  it(`rejects without a session cookie`, () => {
    expect(shouldCaptureReturnVisit(landingRequest(`https://x.test/`))).toBe(
      false
    )
  })

  it(`rejects prefetches, bots, APIs, and assets even with a cookie`, () => {
    expect(
      shouldCaptureReturnVisit(
        landingRequest(`https://x.test/`, {
          ...cookie,
          "sec-purpose": `prefetch`,
        })
      )
    ).toBe(false)
    expect(
      shouldCaptureReturnVisit(
        landingRequest(`https://x.test/`, {
          ...cookie,
          "user-agent": `Googlebot/2.1`,
        })
      )
    ).toBe(false)
    expect(
      shouldCaptureReturnVisit(
        landingRequest(`https://x.test/api/shapes/issues`, cookie)
      )
    ).toBe(false)
    expect(
      shouldCaptureReturnVisit(
        landingRequest(`https://x.test/favicon.ico`, cookie)
      )
    ).toBe(false)
  })

  it(`rejects credential-bearing paths — the token must never land in conversion_events`, () => {
    for (const path of [`/invite/${`ab`.repeat(32)}`, `/support/tok123`]) {
      expect(
        shouldCaptureReturnVisit(landingRequest(`https://x.test${path}`, cookie))
      ).toBe(false)
    }
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
