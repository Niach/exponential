import { describe, expect, it } from "vitest"
import { NotImplementedError } from "./not-implemented"
import {
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
} from "./attachments-upload"

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

  // EXP-936 filled `sessions-compact.ts`: its tests live beside it
  // (`sessions-compact.test.ts`).
})
