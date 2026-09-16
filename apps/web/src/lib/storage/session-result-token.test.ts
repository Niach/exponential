import { describe, expect, it, vi } from "vitest"

vi.stubEnv(`BETTER_AUTH_SECRET`, `session-result-token-test-secret`)

import {
  SESSION_RESULT_TOKEN_TTL_MS,
  mintSessionResultToken,
  verifySessionResultToken,
} from "@/lib/storage/session-result-token"

const SESSION = `00000000-0000-4000-8000-000000000001`
const input = {
  sessionId: SESSION,
  topic: `chatui`,
  label: `web`,
  userId: `user-1`,
}

describe(`session result upload tokens (EXP-879)`, () => {
  it(`round-trips the whole scope and reports the expiry`, () => {
    const now = Date.now()
    const { token, expiresAt } = mintSessionResultToken(input, now)
    expect(expiresAt.getTime()).toBe(now + SESSION_RESULT_TOKEN_TTL_MS)
    expect(SESSION_RESULT_TOKEN_TTL_MS).toBe(10 * 60 * 1000)
    expect(verifySessionResultToken(token, now)).toEqual({
      s: SESSION,
      t: `chatui`,
      l: `web`,
      u: `user-1`,
      exp: now + SESSION_RESULT_TOKEN_TTL_MS,
    })
  })

  it(`rejects an expired token`, () => {
    const now = Date.now()
    const { token } = mintSessionResultToken(input, now)
    expect(
      verifySessionResultToken(token, now + SESSION_RESULT_TOKEN_TTL_MS + 1)
    ).toBeNull()
  })

  it(`rejects tampered payloads and garbage`, () => {
    const { token } = mintSessionResultToken(input)
    const [body, sig] = token.split(`.`)
    // Re-pointing the token at another run (or another label) must not verify.
    const forged = Buffer.from(
      JSON.stringify({
        s: `00000000-0000-4000-8000-00000000beef`,
        t: `chatui`,
        l: `web`,
        u: `user-1`,
        exp: Date.now() + 60_000,
      })
    ).toString(`base64url`)
    for (const bad of [
      `${forged}.${sig}`,
      `${body}.AAAA`,
      body,
      ``,
      `not-a-token`,
    ]) {
      expect(verifySessionResultToken(bad)).toBeNull()
    }
  })

  it(`fails closed when the secret is missing`, () => {
    const { token } = mintSessionResultToken(input)
    vi.stubEnv(`BETTER_AUTH_SECRET`, ``)
    try {
      expect(verifySessionResultToken(token)).toBeNull()
      expect(() => mintSessionResultToken(input)).toThrow(/BETTER_AUTH_SECRET/)
    } finally {
      vi.stubEnv(`BETTER_AUTH_SECRET`, `session-result-token-test-secret`)
    }
  })
})
