import { describe, expect, it, vi } from "vitest"

vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-token-test-secret`)

import {
  ATTACHMENT_TOKEN_TTL_MS,
  mintAttachmentToken,
  verifyAttachmentToken,
} from "@/lib/storage/attachment-token"

const ID = `00000000-0000-4000-8000-000000000001`

describe(`attachment download tokens (EXP-704)`, () => {
  it(`round-trips and reports the expiry`, () => {
    const now = Date.now()
    const { token, expiresAt } = mintAttachmentToken(ID, `user-1`, now)
    expect(expiresAt.getTime()).toBe(now + ATTACHMENT_TOKEN_TTL_MS)
    expect(verifyAttachmentToken(token, now)).toEqual({ attachmentId: ID })
  })

  it(`rejects an expired token`, () => {
    const now = Date.now()
    const { token } = mintAttachmentToken(ID, `user-1`, now)
    expect(
      verifyAttachmentToken(token, now + ATTACHMENT_TOKEN_TTL_MS + 1)
    ).toBeNull()
  })

  it(`rejects tampered payloads and garbage`, () => {
    const { token } = mintAttachmentToken(ID, `user-1`)
    const [body, sig] = token.split(`.`)
    const forged = Buffer.from(
      JSON.stringify({
        a: `00000000-0000-4000-8000-00000000beef`,
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
      expect(verifyAttachmentToken(bad)).toBeNull()
    }
  })

  it(`fails closed when the secret is missing`, () => {
    const { token } = mintAttachmentToken(ID, `user-1`)
    vi.stubEnv(`BETTER_AUTH_SECRET`, ``)
    try {
      expect(verifyAttachmentToken(token)).toBeNull()
      expect(() => mintAttachmentToken(ID, `user-1`)).toThrow(
        /BETTER_AUTH_SECRET/
      )
    } finally {
      vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-token-test-secret`)
    }
  })
})
