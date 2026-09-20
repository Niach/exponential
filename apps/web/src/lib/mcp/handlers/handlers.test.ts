import { describe, expect, it } from "vitest"
import { NotImplementedError } from "./not-implemented"
import {
  countIssueAttachments,
  listIssueAttachments,
} from "./attachments-list"
import {
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
} from "./attachments-upload"

// EXP-988: the contract's handler stubs. Each leaf REPLACES the matching
// assertion here with its own tests when it fills the file; until then the
// stub has to say so loudly rather than return a plausible empty result.
describe(`MCP handler stubs (EXP-988)`, () => {
  it(`attachments_list throws not-implemented (EXP-979)`, async () => {
    await expect(
      listIssueAttachments({ issueId: `i`, limit: 50, offset: 0 })
    ).rejects.toBeInstanceOf(NotImplementedError)
  })

  it(`issues_get's attachmentCount reads 0 until EXP-979 fills it`, async () => {
    await expect(countIssueAttachments(`i`)).resolves.toBe(0)
  })

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
