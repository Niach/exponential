import { describe, expect, it } from "vitest"
import { NotImplementedError } from "./not-implemented"
import {
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
} from "./attachments-upload"
import {
  requestSessionCompaction,
  sessionsCompactRefusals,
} from "./sessions-compact"

// EXP-988: the contract's handler stubs. Each leaf REPLACES the matching
// assertion here with its own tests when it fills the file; until then the
// stub has to say so loudly rather than return a plausible empty result.
describe(`MCP handler stubs (EXP-988)`, () => {
  // EXP-979 filled attachments-list.ts: see attachments-list.test.ts.

  it(`the signed upload halves throw not-implemented (EXP-929)`, async () => {
    await expect(
      mintSignedAttachmentUpload({
        issueId: `i`,
        teamId: `t`,
        boardId: `b`,
        userId: `u`,
        filename: `a.png`,
        contentType: `image/png`,
        origin: `https://x.test`,
      })
    ).rejects.toBeInstanceOf(NotImplementedError)
    await expect(
      finalizeSignedAttachmentUpload({ attachmentId: `a`, userId: `u` })
    ).rejects.toBeInstanceOf(NotImplementedError)
  })

  it(`sessions_compact throws not-implemented (EXP-936) and names its refusals`, async () => {
    await expect(
      requestSessionCompaction({ sessionId: `s`, userId: `u`, reason: `r` })
    ).rejects.toBeInstanceOf(NotImplementedError)
    expect([...sessionsCompactRefusals]).toEqual([
      `too_early`,
      `cooldown`,
      `not_own_session`,
      `unsupported_agent`,
    ])
  })
})
