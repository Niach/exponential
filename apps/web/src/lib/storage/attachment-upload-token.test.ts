import { describe, expect, it, vi } from "vitest"

vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-upload-token-test-secret`)

import {
  ATTACHMENT_UPLOAD_TOKEN_TTL_MS,
  mintAttachmentUploadToken,
  verifyAttachmentUploadToken,
} from "@/lib/storage/attachment-upload-token"

const ATTACHMENT = `00000000-0000-4000-8000-00000000000a`
const ISSUE = `00000000-0000-4000-8000-000000000001`
const input = {
  attachmentId: ATTACHMENT,
  issueId: ISSUE,
  boardId: `board-1`,
  teamId: `team-1`,
  userId: `user-1`,
  filename: `shot.png`,
  contentType: `image/png`,
}

describe(`attachment upload tokens (EXP-929)`, () => {
  it(`round-trips the whole scope and reports the expiry`, () => {
    const now = Date.now()
    const { token, expiresAt } = mintAttachmentUploadToken(input, now)
    expect(expiresAt.getTime()).toBe(now + ATTACHMENT_UPLOAD_TOKEN_TTL_MS)
    // The tool description promises a 10-minute link.
    expect(ATTACHMENT_UPLOAD_TOKEN_TTL_MS).toBe(10 * 60 * 1000)
    expect(verifyAttachmentUploadToken(token, now)).toEqual({
      a: ATTACHMENT,
      i: ISSUE,
      b: `board-1`,
      w: `team-1`,
      u: `user-1`,
      f: `shot.png`,
      c: `image/png`,
      exp: now + ATTACHMENT_UPLOAD_TOKEN_TTL_MS,
    })
  })

  it(`carries the optional comment id only when given`, () => {
    const now = Date.now()
    const { token } = mintAttachmentUploadToken(
      { ...input, commentId: `comment-1` },
      now
    )
    expect(verifyAttachmentUploadToken(token, now)?.m).toBe(`comment-1`)
    const plain = mintAttachmentUploadToken(input, now).token
    expect(`m` in verifyAttachmentUploadToken(plain, now)!).toBe(false)
  })

  it(`rejects an expired token`, () => {
    const now = Date.now()
    const { token } = mintAttachmentUploadToken(input, now)
    expect(
      verifyAttachmentUploadToken(token, now + ATTACHMENT_UPLOAD_TOKEN_TTL_MS + 1)
    ).toBeNull()
  })

  it(`rejects tampered payloads and garbage`, () => {
    const { token } = mintAttachmentUploadToken(input)
    const [body, sig] = token.split(`.`)
    // Re-pointing the token at another issue (or another team's budget) must
    // not verify.
    const forged = Buffer.from(
      JSON.stringify({
        a: ATTACHMENT,
        i: `00000000-0000-4000-8000-00000000beef`,
        b: `board-1`,
        w: `team-1`,
        u: `user-1`,
        f: `shot.png`,
        c: `image/png`,
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
      expect(verifyAttachmentUploadToken(bad)).toBeNull()
    }
  })

  it(`fails closed when the secret is missing`, () => {
    const { token } = mintAttachmentUploadToken(input)
    vi.stubEnv(`BETTER_AUTH_SECRET`, ``)
    try {
      expect(verifyAttachmentUploadToken(token)).toBeNull()
      expect(() => mintAttachmentUploadToken(input)).toThrow(
        /BETTER_AUTH_SECRET/
      )
    } finally {
      vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-upload-token-test-secret`)
    }
  })
})
