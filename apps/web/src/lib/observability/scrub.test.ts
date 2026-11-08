import { describe, expect, it } from "vitest"
import {
  FILTERED,
  scrubAttributes,
  scrubBreadcrumb,
  scrubData,
  scrubEvent,
  scrubLog,
  scrubQueryString,
  scrubUrl,
} from "./scrub"

describe(`scrubUrl / scrubQueryString`, () => {
  it(`filters every sensitive query key and keeps the rest`, () => {
    expect(
      scrubUrl(`https://app.example/api/x?token=abc&board=1&api_key=k&sessionId=s#frag`)
    ).toBe(
      `https://app.example/api/x?token=${FILTERED}&board=1&api_key=${FILTERED}&sessionId=${FILTERED}#frag`
    )
    for (const key of [`secret`, `password`, `ticket`, `dsn`, `Session`, `KEY`]) {
      expect(scrubQueryString(`?${key}=v&ok=1`)).toBe(`?${key}=${FILTERED}&ok=1`)
    }
  })

  it(`leaves URLs without a query alone and decodes encoded keys`, () => {
    expect(scrubUrl(`/api/shapes/issues`)).toBe(`/api/shapes/issues`)
    expect(scrubQueryString(`a%5Ftoken=1`)).toBe(`a%5Ftoken=${FILTERED}`)
  })
})

describe(`scrubEvent`, () => {
  const event = {
    request: {
      url: `https://app.example/support/x?ticket=t1`,
      query_string: `ticket=t1&page=2`,
      headers: {
        Authorization: `Bearer secret`,
        Cookie: `better-auth.session_token=x`,
        "Set-Cookie": `a=b`,
        "X-Api-Key": `expu_1`,
        "User-Agent": `UA`,
      },
      cookies: { session: `x` },
      data: JSON.stringify({ password: `hunter2`, title: `hi` }),
    },
    user: { id: `u1`, email: `a@b.c`, ip_address: `1.2.3.4`, username: `x` },
    extra: { nested: { refreshToken: `r`, ok: 1 } },
    breadcrumbs: [{ category: `fetch`, data: { url: `/api/x?key=1`, method: `GET` } }],
    spans: [
      {
        description: `GET /api/shapes/issues?session=abc`,
        data: { "url.full": `https://h/p?password=1`, "http.request.header.authorization": `x` },
      },
    ],
  }

  const out = scrubEvent(event)

  it(`drops credentials headers and cookies, filters bodies`, () => {
    expect(out.request?.headers).toEqual({ "User-Agent": `UA` })
    expect(out.request?.cookies).toBeUndefined()
    expect(out.request?.url).toBe(`https://app.example/support/x?ticket=${FILTERED}`)
    expect(out.request?.query_string).toBe(`ticket=${FILTERED}&page=2`)
    expect(JSON.parse(out.request?.data as string)).toEqual({ password: FILTERED, title: `hi` })
  })

  it(`keeps only the user id`, () => {
    expect(out.user).toEqual({ id: `u1` })
  })

  it(`scrubs extra, breadcrumbs and spans`, () => {
    expect(out.extra).toEqual({ nested: { refreshToken: FILTERED, ok: 1 } })
    expect(out.breadcrumbs?.[0].data).toEqual({ url: `/api/x?key=${FILTERED}`, method: `GET` })
    expect(out.spans?.[0].description).toBe(`GET /api/shapes/issues?session=${FILTERED}`)
    expect(out.spans?.[0].data).toEqual({ "url.full": `https://h/p?password=${FILTERED}` })
  })

  it(`never mutates its input`, () => {
    expect(event.request.headers.Authorization).toBe(`Bearer secret`)
    expect(event.user.email).toBe(`a@b.c`)
  })

  it(`passes null through (a dropped event stays dropped)`, () => {
    expect(scrubEvent(null)).toBeNull()
    expect(scrubBreadcrumb(null)).toBeNull()
  })
})

describe(`scrubBreadcrumb / scrubLog / scrubAttributes / scrubData`, () => {
  it(`scrubs navigation from/to and console messages`, () => {
    expect(
      scrubBreadcrumb({ category: `navigation`, data: { from: `/a?token=1`, to: `/b` } }).data
    ).toEqual({ from: `/a?token=${FILTERED}`, to: `/b` })
    expect(scrubBreadcrumb({ message: `failed GET /x?secret=1 now` }).message).toBe(
      `failed GET /x?secret=${FILTERED} now`
    )
  })

  it(`scrubs log attributes and messages`, () => {
    const log = scrubLog({
      message: `upstream /api?dsn=https://k@h/1`,
      attributes: { "user.session": `s`, count: 3 },
    })
    expect(log.message).toBe(`upstream /api?dsn=${FILTERED}`)
    expect(log.attributes).toEqual({ "user.session": FILTERED, count: 3 })
  })

  it(`drops sensitive header attributes`, () => {
    expect(
      scrubAttributes({
        "http.request.header.cookie": `c`,
        "http.request.header.x_api_key": `k`,
        "http.request.header.accept": `*/*`,
      })
    ).toEqual({ "http.request.header.accept": `*/*` })
  })

  it(`walks arrays and nested objects`, () => {
    expect(scrubData([{ apiKey: 1 }, { list: [{ clientSecret: 2, x: 3 }] }])).toEqual([
      { apiKey: FILTERED },
      { list: [{ clientSecret: FILTERED, x: 3 }] },
    ])
  })
})
