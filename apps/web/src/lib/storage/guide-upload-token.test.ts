import { describe, expect, it, vi } from "vitest"

vi.stubEnv(`BETTER_AUTH_SECRET`, `guide-upload-token-test-secret`)

import {
  GUIDE_UPLOAD_TOKEN_TTL_MS,
  mintGuideUploadToken,
  verifyGuideUploadToken,
} from "@/lib/storage/guide-upload-token"

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
    const { token, expiresAt } = mintGuideUploadToken(input, now)
    expect(expiresAt.getTime()).toBe(now + GUIDE_UPLOAD_TOKEN_TTL_MS)
    expect(GUIDE_UPLOAD_TOKEN_TTL_MS).toBe(10 * 60 * 1000)
    expect(verifyGuideUploadToken(token, now)).toEqual({
      s: SESSION,
      t: `chatui`,
      l: `web`,
      u: `user-1`,
      exp: now + GUIDE_UPLOAD_TOKEN_TTL_MS,
    })
  })

  it(`rejects an expired token`, () => {
    const now = Date.now()
    const { token } = mintGuideUploadToken(input, now)
    expect(
      verifyGuideUploadToken(token, now + GUIDE_UPLOAD_TOKEN_TTL_MS + 1)
    ).toBeNull()
  })

  it(`rejects tampered payloads and garbage`, () => {
    const { token } = mintGuideUploadToken(input)
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
      expect(verifyGuideUploadToken(bad)).toBeNull()
    }
  })

  it(`fails closed when the secret is missing`, () => {
    const { token } = mintGuideUploadToken(input)
    vi.stubEnv(`BETTER_AUTH_SECRET`, ``)
    try {
      expect(verifyGuideUploadToken(token)).toBeNull()
      expect(() => mintGuideUploadToken(input)).toThrow(/BETTER_AUTH_SECRET/)
    } finally {
      vi.stubEnv(`BETTER_AUTH_SECRET`, `guide-upload-token-test-secret`)
    }
  })

  // EXP-1280: a plan/question card's picture.
  it(`round-trips a card target and refuses a re-pointed one`, () => {
    const now = Date.now()
    const MESSAGE = `00000000-0000-4000-8000-0000000000aa`
    const { token } = mintGuideUploadToken(
      { sessionId: SESSION, topic: ``, label: `grid`, userId: `user-1`, card: { messageId: MESSAGE } },
      now
    )
    expect(verifyGuideUploadToken(token, now)).toEqual({
      s: SESSION,
      t: ``,
      l: `grid`,
      u: `user-1`,
      exp: now + GUIDE_UPLOAD_TOKEN_TTL_MS,
      m: MESSAGE,
    })
    const [, sig] = token.split(`.`)
    const forged = Buffer.from(
      JSON.stringify({ s: SESSION, t: ``, l: `grid`, u: `user-1`, exp: now + 60_000, m: `other` })
    ).toString(`base64url`)
    expect(verifyGuideUploadToken(`${forged}.${sig}`, now)).toBeNull()
  })
})
