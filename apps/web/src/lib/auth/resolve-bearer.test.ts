import { beforeEach, describe, expect, it, vi } from "vitest"

// Regression lock for the cross-account leak: Better Auth's `session_data`
// cookie cache is trusted over the bearer inside getSession, so a token
// (mobile bearer / `expu_` api key) request that ALSO carries a stale cookie
// from a previous user must resolve identity from the TOKEN alone. resolveSession
// strips the Cookie header whenever an Authorization header is present; cookie-only
// (web) requests keep their cookie untouched.

const h = vi.hoisted(() => {
  const state = {
    lastHeaders: undefined as Headers | undefined,
    user: { id: `user-token` } as { id: string } | null,
    calls: 0,
    throwNext: false,
  }
  return { state }
})

vi.mock(`@/lib/auth`, () => ({
  auth: {
    api: {
      getSession: vi.fn(async ({ headers }: { headers: Headers }) => {
        h.state.calls++
        h.state.lastHeaders = headers
        if (h.state.throwNext) {
          h.state.throwNext = false
          throw new Error(`db down`)
        }
        return h.state.user ? { user: h.state.user } : null
      }),
    },
  },
}))

import {
  invalidateSessionCache,
  resolveSession,
  resolveSessionUserId,
} from "@/lib/auth/resolve-bearer"

beforeEach(() => {
  h.state.lastHeaders = undefined
  h.state.user = { id: `user-token` }
  h.state.calls = 0
  h.state.throwNext = false
  // The REV2-7 session cache is a module singleton — clear it so each
  // test starts cold (several tests reuse the same bearer literal).
  invalidateSessionCache()
})

describe(`resolveSession bearer/cookie isolation`, () => {

  it(`strips the Cookie header when an Authorization bearer is present`, async () => {
    const request = new Request(`https://x/api/shapes/issues`, {
      headers: {
        authorization: `Bearer sometoken`,
        cookie: `__Secure-better-auth.session_data=stale-other-user`,
      },
    })
    await resolveSession(request)
    expect(h.state.lastHeaders?.get(`cookie`)).toBeNull()
    // The bearer itself must survive — it's what the session resolves from.
    expect(h.state.lastHeaders?.get(`authorization`)).toBe(`Bearer sometoken`)
  })

  it(`strips the Cookie header for an expu_ api key too`, async () => {
    const request = new Request(`https://x/api/trpc/issues.list`, {
      headers: {
        authorization: `Bearer expu_abc123`,
        cookie: `__Secure-better-auth.session_data=stale-other-user`,
      },
    })
    await resolveSession(request)
    expect(h.state.lastHeaders?.get(`cookie`)).toBeNull()
  })

  it(`strips the Cookie header for an x-api-key request`, async () => {
    const request = new Request(`https://x/api/shapes/issues`, {
      headers: {
        "x-api-key": `expu_abc123`,
        cookie: `__Secure-better-auth.session_data=stale-other-user`,
      },
    })
    await resolveSession(request)
    expect(h.state.lastHeaders?.get(`cookie`)).toBeNull()
    expect(h.state.lastHeaders?.get(`x-api-key`)).toBe(`expu_abc123`)
  })

  it(`preserves the Cookie header for a cookie-only (web) request`, async () => {
    const request = new Request(`https://x/api/trpc/issues.list`, {
      headers: { cookie: `__Secure-better-auth.session_token=web-user` },
    })
    await resolveSession(request)
    expect(h.state.lastHeaders?.get(`cookie`)).toBe(
      `__Secure-better-auth.session_token=web-user`
    )
  })

  it(`does not mutate the original request headers`, async () => {
    const request = new Request(`https://x/api/shapes/issues`, {
      headers: { authorization: `Bearer t`, cookie: `session_data=x` },
    })
    await resolveSession(request)
    expect(request.headers.get(`cookie`)).toBe(`session_data=x`)
  })

  it(`returns null when getSession yields no user`, async () => {
    h.state.user = null
    const request = new Request(`https://x/api/shapes/issues`, {
      headers: { authorization: `Bearer t` },
    })
    expect(await resolveSession(request)).toBeNull()
    expect(await resolveSessionUserId(request)).toBeNull()
  })
})

// REV2-7: token-credentialed sessions are cached ~30s in-process, because the
// cookie strip above deliberately bypasses Better Auth's cookieCache — before
// this cache every one of a native client's 14 shape long-poll renewals per
// cycle was a real session/apikey DB lookup.
describe(`resolveSession token-session cache (REV2-7)`, () => {
  it(`caches a token-credentialed session within the TTL`, async () => {
    const request = () =>
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer sometoken` },
      })
    expect((await resolveSession(request()))?.user?.id).toBe(`user-token`)
    expect((await resolveSession(request()))?.user?.id).toBe(`user-token`)
    expect(h.state.calls).toBe(1)
  })

  it(`coalesces concurrent lookups for the same token`, async () => {
    const request = () =>
      new Request(`https://x/api/shapes/issues`, {
        headers: { "x-api-key": `expu_abc123` },
      })
    const [a, b] = await Promise.all([
      resolveSession(request()),
      resolveSession(request()),
    ])
    expect(a?.user?.id).toBe(`user-token`)
    expect(b?.user?.id).toBe(`user-token`)
    expect(h.state.calls).toBe(1)
  })

  it(`different tokens resolve independently`, async () => {
    await resolveSession(
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer token-a` },
      })
    )
    await resolveSession(
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer token-b` },
      })
    )
    expect(h.state.calls).toBe(2)
  })

  it(`never caches a null resolution (dead/revoked token)`, async () => {
    h.state.user = null
    const request = () =>
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer deadtoken` },
      })
    expect(await resolveSession(request())).toBeNull()
    expect(await resolveSession(request())).toBeNull()
    expect(h.state.calls).toBe(2)
  })

  it(`never caches a thrown getSession (transient DB error)`, async () => {
    h.state.throwNext = true
    const request = () =>
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer sometoken` },
      })
    // The error is normalized to null (pre-existing behavior) and NOT cached…
    expect(await resolveSession(request())).toBeNull()
    // …so the next call re-resolves and succeeds.
    expect((await resolveSession(request()))?.user?.id).toBe(`user-token`)
    expect(h.state.calls).toBe(2)
  })

  it(`never caches cookie-only (web) requests — cookieCache covers those`, async () => {
    const request = () =>
      new Request(`https://x/api/trpc/issues.list`, {
        headers: { cookie: `__Secure-better-auth.session_token=web-user` },
      })
    await resolveSession(request())
    await resolveSession(request())
    expect(h.state.calls).toBe(2)
  })

  it(`invalidateSessionCache drops cached token sessions`, async () => {
    const request = () =>
      new Request(`https://x/api/shapes/issues`, {
        headers: { authorization: `Bearer sometoken` },
      })
    await resolveSession(request())
    invalidateSessionCache()
    await resolveSession(request())
    expect(h.state.calls).toBe(2)
  })
})
