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
  }
  return { state }
})

vi.mock(`@/lib/auth`, () => ({
  auth: {
    api: {
      getSession: vi.fn(async ({ headers }: { headers: Headers }) => {
        h.state.lastHeaders = headers
        return h.state.user ? { user: h.state.user } : null
      }),
    },
  },
}))

import { resolveSession, resolveSessionUserId } from "@/lib/auth/resolve-bearer"

describe(`resolveSession bearer/cookie isolation`, () => {
  beforeEach(() => {
    h.state.lastHeaders = undefined
    h.state.user = { id: `user-token` }
  })

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
