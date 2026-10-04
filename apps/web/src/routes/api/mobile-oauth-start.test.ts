// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const h = vi.hoisted(() => ({ signInSocial: vi.fn(), linkSocialAccount: vi.fn() }))

vi.mock(`@/lib/auth`, () => ({
  auth: {
    api: { signInSocial: h.signInSocial, linkSocialAccount: h.linkSocialAccount },
  },
}))
vi.mock(`@/db/connection`, () => ({ db: {} }))

import { Route } from "@/routes/api/mobile-oauth-start"

type Handler = (args: { request: Request }) => Promise<Response>
const GET = (
  Route as unknown as { options: { server: { handlers: { GET: Handler } } } }
).options.server.handlers.GET

const CHALLENGE = `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`
const start = (query: string) =>
  GET({
    request: new Request(
      `https://app.example/api/mobile-oauth-start?${query}&code_challenge=${CHALLENGE}&code_challenge_method=S256`
    ),
  })

describe(`mobile-oauth-start: GitHub sign-in follows GITHUB_LOGIN_ENABLED`, () => {
  beforeEach(() => {
    h.signInSocial.mockReset()
    for (const [name, value] of Object.entries({
      GITHUB_APP_ID: `1`,
      GITHUB_APP_PRIVATE_KEY: `pem`,
      GITHUB_APP_SLUG: `exp`,
      GITHUB_APP_CLIENT_ID: `gh-client`,
      GITHUB_APP_CLIENT_SECRET: `gh-secret`,
    }))
      vi.stubEnv(name, value)
  })

  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it(`refuses provider=github onto the return route while login is off`, async () => {
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `false`)
    const res = await start(`provider=github`)
    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toBe(
      `https://app.example/api/mobile-oauth-return?error=provider_disabled`
    )
    expect(h.signInSocial).not.toHaveBeenCalled()
  })

  it(`starts the sign-in once login is on`, async () => {
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `true`)
    h.signInSocial.mockResolvedValue(
      Response.json({ url: `https://github.com/login/oauth/authorize?x=1`, redirect: true })
    )
    const res = await start(`provider=github`)
    expect(h.signInSocial).toHaveBeenCalledTimes(1)
    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toBe(`https://github.com/login/oauth/authorize?x=1`)
  })

  it(`other providers are untouched`, async () => {
    vi.stubEnv(`GITHUB_LOGIN_ENABLED`, `false`)
    h.signInSocial.mockResolvedValue(
      Response.json({ url: `https://accounts.google.com/o/oauth2/auth`, redirect: true })
    )
    const res = await start(`provider=google`)
    expect(h.signInSocial).toHaveBeenCalledTimes(1)
    expect(res.headers.get(`location`)).toBe(`https://accounts.google.com/o/oauth2/auth`)
  })
})
