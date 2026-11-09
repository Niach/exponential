import { gzipSync } from "node:zlib"
import { describe, expect, it, vi } from "vitest"
import {
  envelopeDsn,
  sentryAuthKey,
  handleMonitoringTunnel,
  MAX_ENVELOPE_BYTES,
  upstreamHeaders,
  type TunnelDeps,
} from "./tunnel"

const ENV = {
  GLITCHTIP_INTERNAL_URL: `http://glitchtip-web:8000`,
  SENTRY_DSN_WEB: `https://webkey@glitchtip.example/1`,
  SENTRY_DSN_IOS: `https://ioskey@glitchtip.example/5`,
}

const envelope = (dsn: string | null, payload = `{"type":"event"}\n{"message":"x"}`) =>
  `${JSON.stringify(dsn ? { dsn, sent_at: `2026-11-09T00:00:00Z` } : { sent_at: `x` })}\n${payload}`

function deps(overrides: Partial<TunnelDeps> = {}) {
  const fetch = vi.fn(async (_url: string, _init: RequestInit) => new Response(null, { status: 200 }))
  const takeToken = vi.fn((_key: string) => ({ ok: true }) as const)
  const d: TunnelDeps = {
    env: ENV,
    fetch,
    resolveUserId: async () => null,
    clientIp: () => `203.0.113.7`,
    takeToken,
    ...overrides,
  }
  return { d, fetch, takeToken }
}

const post = (body: BodyInit, headers: Record<string, string> = {}) =>
  new Request(`https://app.example/api/monitoring`, { method: `POST`, body, headers })

describe(`the DSN allowlist`, () => {
  it(`(1) resolves the envelope header's dsn; forwards with the SERVER's key, internal host`, async () => {
    const { d, fetch } = deps()
    const res = await handleMonitoringTunnel(
      post(envelope(`https://clientkey@evil.example/1`)),
      d
    )
    expect(res.status).toBe(200)
    expect(fetch).toHaveBeenCalledOnce()
    expect(fetch.mock.calls[0][0]).toBe(
      `http://glitchtip-web:8000/api/1/envelope/?sentry_key=webkey&sentry_version=7`
    )
  })

  it(`(2) without a dsn header, resolves the X-Sentry-Auth or ?sentry_key= key`, async () => {
    // Forwarded with the configured key of the resolved project (here the same).
    const auth = deps()
    const res = await handleMonitoringTunnel(
      post(envelope(null), {
        "x-sentry-auth": `Sentry sentry_version=7, sentry_client=sentry.java.android/8, sentry_key=ioskey`,
      }),
      auth.d
    )
    expect(res.status).toBe(200)
    expect(auth.fetch.mock.calls[0][0]).toBe(
      `http://glitchtip-web:8000/api/5/envelope/?sentry_key=ioskey&sentry_version=7`
    )
    const query = deps()
    const req = new Request(`https://app.example/api/monitoring?sentry_key=webkey`, {
      method: `POST`,
      body: envelope(null),
    })
    expect((await handleMonitoringTunnel(req, query.d)).status).toBe(200)
    expect(query.fetch.mock.calls[0][0]).toContain(`/api/1/envelope/?sentry_key=webkey`)
  })

  it(`(3) falls back to ?platform= and that platform's configured DSN`, async () => {
    const { d, fetch } = deps()
    const req = new Request(`https://app.example/api/monitoring?platform=ios`, {
      method: `POST`,
      body: envelope(null),
    })
    expect((await handleMonitoringTunnel(req, d)).status).toBe(200)
    expect(fetch.mock.calls[0][0]).toBe(
      `http://glitchtip-web:8000/api/5/envelope/?sentry_key=ioskey&sentry_version=7`
    )
  })

  it(`403s anything that matches no configured project`, async () => {
    const cases: Request[] = [
      post(envelope(`https://webkey@glitchtip.example/99`)),
      post(envelope(null)),
      post(`not json\n{}`),
      post(envelope(null), { "x-sentry-auth": `Sentry sentry_key=unknown` }),
      new Request(`https://app.example/api/monitoring?platform=android`, {
        method: `POST`,
        body: envelope(null),
      }),
      new Request(`https://app.example/api/monitoring?platform=nope`, {
        method: `POST`,
        body: envelope(null),
      }),
    ]
    for (const req of cases) {
      const { d, fetch } = deps()
      expect((await handleMonitoringTunnel(req, d)).status).toBe(403)
      expect(fetch).not.toHaveBeenCalled()
    }
  })

  it(`passes GlitchTip's status through, never its body`, async () => {
    const { d } = deps({
      fetch: async () => new Response(`denied`, { status: 403 }),
    })
    const res = await handleMonitoringTunnel(post(envelope(ENV.SENTRY_DSN_IOS)), d)
    expect(res.status).toBe(403)
    expect(await res.text()).toBe(``)
  })

  it(`is 404 when diagnostics are not configured`, async () => {
    const { d } = deps({ env: {} })
    expect((await handleMonitoringTunnel(post(envelope(ENV.SENTRY_DSN_WEB)), d)).status).toBe(404)
  })

  it(`reads the header of a gzipped envelope and forwards it still compressed`, async () => {
    const { d, fetch } = deps()
    const body = gzipSync(envelope(ENV.SENTRY_DSN_IOS))
    const res = await handleMonitoringTunnel(post(body, { "content-encoding": `gzip` }), d)
    expect(res.status).toBe(200)
    const init = fetch.mock.calls[0][1]
    expect(new Headers(init.headers).get(`content-encoding`)).toBe(`gzip`)
    expect(new Headers(init.headers).get(`x-sentry-auth`)).toContain(`sentry_key=ioskey`)
    expect(Buffer.from(init.body as Uint8Array).equals(body)).toBe(true)
    expect(envelopeDsn(body, `br`)).toBeNull()
  })
})

describe(`sentryAuthKey`, () => {
  it(`reads the key anywhere in the header`, () => {
    expect(sentryAuthKey(`Sentry sentry_key=abc, sentry_version=7`)).toBe(`abc`)
    expect(sentryAuthKey(`Sentry sentry_version=7,sentry_key=def`)).toBe(`def`)
    expect(sentryAuthKey(`Sentry sentry_version=7`)).toBeNull()
    expect(sentryAuthKey(null)).toBeNull()
  })
})

describe(`the size cap`, () => {
  it(`refuses a body past ~1 MB, declared or streamed`, async () => {
    const big = envelope(ENV.SENTRY_DSN_WEB, `x`.repeat(MAX_ENVELOPE_BYTES))
    const { d, fetch } = deps()
    expect((await handleMonitoringTunnel(post(big), d)).status).toBe(413)
    const stream = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(new TextEncoder().encode(big))
        c.close()
      },
    })
    const streamed = new Request(`https://app.example/api/monitoring`, {
      method: `POST`,
      body: stream,
      // @ts-expect-error — undici needs it for a stream body
      duplex: `half`,
    })
    expect((await handleMonitoringTunnel(streamed, d)).status).toBe(413)
    expect(fetch).not.toHaveBeenCalled()
  })

  it(`refuses a gzip envelope that inflates past the cap (zip bomb)`, async () => {
    const bomb = gzipSync(envelope(ENV.SENTRY_DSN_WEB, `\0`.repeat(MAX_ENVELOPE_BYTES * 4)))
    expect(bomb.byteLength).toBeLessThan(MAX_ENVELOPE_BYTES)
    const { d, fetch } = deps()
    const res = await handleMonitoringTunnel(post(bomb, { "content-encoding": `gzip` }), d)
    expect(res.status).toBe(413)
    expect(fetch).not.toHaveBeenCalled()
  })

  it(`accepts a body just under the cap`, async () => {
    const { d } = deps()
    const head = envelope(ENV.SENTRY_DSN_WEB, ``)
    const body = head + `x`.repeat(MAX_ENVELOPE_BYTES - head.length)
    expect((await handleMonitoringTunnel(post(body), d)).status).toBe(200)
  })
})

describe(`IP stripping`, () => {
  it(`forwards a fresh header set: no client address, cookie or credential`, async () => {
    const { d, fetch } = deps()
    await handleMonitoringTunnel(
      post(envelope(ENV.SENTRY_DSN_WEB), {
        "x-forwarded-for": `198.51.100.1, 203.0.113.7`,
        "x-real-ip": `198.51.100.1`,
        forwarded: `for=198.51.100.1`,
        "cf-connecting-ip": `198.51.100.1`,
        cookie: `better-auth.session_token=x`,
        authorization: `Bearer y`,
        "user-agent": `UA`,
      }),
      d
    )
    const headers = new Headers(fetch.mock.calls[0][1].headers)
    expect([...headers.keys()].sort()).toEqual([`content-type`, `x-sentry-auth`])
    expect(headers.get(`content-type`)).toBe(`application/x-sentry-envelope`)
    expect(headers.get(`x-sentry-auth`)).toContain(`sentry_key=webkey`)
  })

  it(`upstreamHeaders keeps only a known content-encoding`, () => {
    const h = upstreamHeaders(post(`x`, { "content-encoding": `evil`, "x-forwarded-for": `1.2.3.4` }))
    expect([...h.keys()]).toEqual([`content-type`])
  })
})

describe(`the rate limit`, () => {
  it(`keys on the user when signed in, else the client IP`, async () => {
    const anon = deps()
    await handleMonitoringTunnel(post(envelope(ENV.SENTRY_DSN_WEB)), anon.d)
    expect(anon.takeToken).toHaveBeenCalledWith(`ip:203.0.113.7`)
    const user = deps({ resolveUserId: async () => `user-1` })
    await handleMonitoringTunnel(post(envelope(ENV.SENTRY_DSN_WEB)), user.d)
    expect(user.takeToken).toHaveBeenCalledWith(`u:user-1`)
  })

  it(`answers 429 with Retry-After and forwards nothing`, async () => {
    const { d, fetch } = deps({
      takeToken: () => ({ ok: false, retryAfterSeconds: 30 }),
    })
    const res = await handleMonitoringTunnel(post(envelope(ENV.SENTRY_DSN_WEB)), d)
    expect(res.status).toBe(429)
    expect(res.headers.get(`retry-after`)).toBe(`30`)
    expect(fetch).not.toHaveBeenCalled()
  })
})
