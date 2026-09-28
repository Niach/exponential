// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import {
  generateInstallToken,
  hashInstallToken,
  isInstallTokenShape,
} from "./cli-install-token"
import {
  redeemInstallTokenRequest,
  type RedeemDeps,
} from "./cli-install-token-redeem"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

describe(`install token shape (EXP-1111)`, () => {
  it(`mints expi_ + 32 random bytes and stores only the sha256 hex`, () => {
    const a = generateInstallToken()
    const b = generateInstallToken()
    expect(a.token).toMatch(/^expi_[A-Za-z0-9_-]{43}$/)
    expect(a.token).not.toBe(b.token)
    expect(a.tokenHash).toMatch(/^[0-9a-f]{64}$/)
    expect(a.tokenHash).toBe(hashInstallToken(a.token))
    expect(isInstallTokenShape(a.token)).toBe(true)
    expect(isInstallTokenShape(`expu_${a.token.slice(5)}`)).toBe(false)
    expect(isInstallTokenShape(`expi_short`)).toBe(false)
    expect(isInstallTokenShape(42)).toBe(false)
  })
})

function redeem(body: unknown, deps: Partial<RedeemDeps> = {}) {
  const claimed = new Set<string>()
  const valid = generateInstallToken().token
  const full: RedeemDeps = {
    // Single use, like the conditional UPDATE.
    claim: async (token) => {
      if (token !== valid || claimed.has(token)) return null
      claimed.add(token)
      return `user-1`
    },
    createSession: async (userId) => ({ token: `session-for-${userId}` }),
    allow: () => true,
    ...deps,
  }
  const send = (payload: unknown) =>
    redeemInstallTokenRequest(
      new Request(`http://localhost/api/cli/install-token/redeem`, {
        method: `POST`,
        headers: { "content-type": `application/json` },
        body: typeof payload === `string` ? payload : JSON.stringify(payload),
      }),
      full
    )
  return {
    send: () => send(body === `VALID` ? { token: valid } : body),
    sendAgain: () => send({ token: valid }),
  }
}

describe(`POST /api/cli/install-token/redeem`, () => {
  it(`trades a valid token once for a Bearer session token`, async () => {
    const { send, sendAgain } = redeem(`VALID`)
    const first = await send()
    expect(first.status).toBe(200)
    expect(await first.json()).toEqual({
      access_token: `session-for-user-1`,
      token_type: `Bearer`,
    })
    expect(first.headers.get(`cache-control`)).toBe(`no-store`)
    const second = await sendAgain()
    expect(second.status).toBe(400)
    expect(await second.json()).toEqual({ error: `invalid_token` })
  })

  it(`answers invalid_token for junk, unknown tokens and bad JSON`, async () => {
    for (const body of [{ token: `expi_nope` }, {}, `not json`, null]) {
      const res = await redeem(body).send()
      expect(res.status).toBe(400)
      expect(await res.json()).toEqual({ error: `invalid_token` })
    }
  })

  it(`is rate-limited per IP`, async () => {
    const res = await redeem(`VALID`, { allow: () => false }).send()
    expect(res.status).toBe(429)
  })
})
