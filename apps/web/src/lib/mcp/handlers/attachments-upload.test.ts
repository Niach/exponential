import { beforeEach, describe, expect, it, vi } from "vitest"

vi.stubEnv(`BETTER_AUTH_SECRET`, `attachments-upload-handler-test-secret`)

// EXP-929: the signed half of `exponential_attachments_upload`. The registry
// (tools.ts) owns access; these tests cover what the handler adds — the
// commentId gate, the token's scope + curl line, and the finalize call's
// ownership check + result shape. `finalizeAttachmentUpload` itself is
// EXP-955's (lib/attachments/finalize.ts) and is mocked here.

const h = vi.hoisted(() => {
  const rows: { current: unknown[] } = { current: [] }
  const select = vi.fn()
  const finalizeAttachmentUpload = vi.fn()
  const getIssueTeamContext = vi.fn()
  const assertTeamMember = vi.fn()
  return {
    rows,
    select,
    finalizeAttachmentUpload,
    getIssueTeamContext,
    assertTeamMember,
  }
})

function builder(rows: () => unknown[]) {
  const query: Record<string, unknown> = {}
  for (const method of [`from`, `where`, `limit`]) {
    query[method] = vi.fn(() => query)
  }
  ;(query as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(rows()).then(resolve, reject)
  return query
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: (...args: unknown[]) => {
      h.select(...args)
      return builder(() => h.rows.current)
    },
  },
  pool: {},
}))

vi.mock(`@/lib/attachments/finalize`, () => ({
  finalizeAttachmentUpload: h.finalizeAttachmentUpload,
}))

vi.mock(`@/lib/team-membership`, () => ({
  getIssueTeamContext: h.getIssueTeamContext,
  assertTeamMember: h.assertTeamMember,
}))

import {
  finalizeSignedAttachmentUpload,
  finalizedAttachmentResult,
  mintSignedAttachmentUpload,
} from "./attachments-upload"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"
import { verifyAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"

const ISSUE = `00000000-0000-4000-8000-000000000001`
const ATTACHMENT = `00000000-0000-4000-8000-00000000000a`
const mintInput = {
  issueId: ISSUE,
  teamId: `team-1`,
  boardId: `board-1`,
  userId: `user-1`,
  filename: `shot.png`,
  contentType: `image/png`,
  origin: `https://x.test`,
}

function attachmentRow(overrides: Record<string, unknown> = {}) {
  return {
    id: ATTACHMENT,
    url: `/api/attachments/${ATTACHMENT}`,
    filename: `shot.png`,
    contentType: `image/png`,
    sizeBytes: 33_000,
    width: 800,
    height: 600,
    durationMs: null,
    uploaderId: `user-1`,
    issueId: ISSUE,
    ...overrides,
  }
}

beforeEach(() => {
  h.rows.current = []
  h.select.mockClear()
  h.finalizeAttachmentUpload.mockReset()
  h.getIssueTeamContext.mockReset()
  h.getIssueTeamContext.mockResolvedValue({
    issueId: ISSUE,
    boardId: `board-1`,
    teamId: `team-1`,
  })
  h.assertTeamMember.mockReset()
  h.assertTeamMember.mockResolvedValue({ role: `member` })
})

function boardGrant(boardId: string, teamId: string): McpAccess {
  return {
    full: false,
    fullTeamIds: new Set(),
    grantedBoardIds: new Set([boardId]),
    visibleTeamIds: new Set([teamId]),
  }
}

describe(`mintSignedAttachmentUpload (EXP-929)`, () => {
  it(`returns the sessions_results grant shape: a scoped 10-minute URL and a ready curl line, and writes nothing`, async () => {
    const before = Date.now()
    const result = await mintSignedAttachmentUpload(mintInput)
    expect(Object.keys(result).sort()).toEqual([
      `attachmentId`,
      `curl`,
      `expiresAt`,
      `uploadUrl`,
    ])
    expect(result.uploadUrl.startsWith(`https://x.test/api/attachment-uploads/`)).toBe(
      true
    )
    expect(result.curl).toBe(`curl -sS -T 'shot.png' "${result.uploadUrl}"`)
    const expires = Date.parse(result.expiresAt)
    expect(expires).toBeGreaterThanOrEqual(before + 10 * 60 * 1000)
    expect(expires).toBeLessThanOrEqual(Date.now() + 10 * 60 * 1000)
    // The token carries the whole scope the route writes the row from, and the
    // attachment id it announces is the one the row will get.
    const token = result.uploadUrl.split(`/`).pop()!
    expect(verifyAttachmentUploadToken(token)).toMatchObject({
      a: result.attachmentId,
      i: ISSUE,
      b: `board-1`,
      w: `team-1`,
      u: `user-1`,
      f: `shot.png`,
      c: `image/png`,
    })
    // No commentId → no db round-trip, no row: an abandoned mint leaves
    // nothing behind.
    expect(h.select).not.toHaveBeenCalled()
  })

  it(`single-quotes the filename in the curl line`, async () => {
    const result = await mintSignedAttachmentUpload({
      ...mintInput,
      filename: `Danny's $shot (1).png`,
    })
    expect(result.curl.startsWith(`curl -sS -T 'Danny'\\''s $shot (1).png' "`)).toBe(
      true
    )
  })

  it(`binds a commentId only when the comment is on this issue and the caller wrote it`, async () => {
    h.rows.current = [{ issueId: ISSUE, authorId: `user-1` }]
    const result = await mintSignedAttachmentUpload({
      ...mintInput,
      commentId: `comment-1`,
    })
    const token = result.uploadUrl.split(`/`).pop()!
    expect(verifyAttachmentUploadToken(token)?.m).toBe(`comment-1`)
  })

  it(`refuses a commentId from another issue or another author`, async () => {
    h.rows.current = [{ issueId: `other-issue`, authorId: `user-1` }]
    await expect(
      mintSignedAttachmentUpload({ ...mintInput, commentId: `comment-1` })
    ).rejects.toThrow(/comment on this issue/)
    h.rows.current = [{ issueId: ISSUE, authorId: `user-2` }]
    await expect(
      mintSignedAttachmentUpload({ ...mintInput, commentId: `comment-1` })
    ).rejects.toThrow(/comment's author/)
    h.rows.current = []
    await expect(
      mintSignedAttachmentUpload({ ...mintInput, commentId: `comment-1` })
    ).rejects.toThrow(/comment on this issue/)
  })
})

describe(`finalizeSignedAttachmentUpload (EXP-929)`, () => {
  it(`runs finalizeAttachmentUpload on the caller's row and returns the inline path's shape with markdown`, async () => {
    h.rows.current = [attachmentRow()]
    h.finalizeAttachmentUpload.mockResolvedValue(attachmentRow())
    const result = await finalizeSignedAttachmentUpload({
      attachmentId: ATTACHMENT,
      userId: `user-1`,
      access: FULL_ACCESS,
    })
    expect(h.getIssueTeamContext).toHaveBeenCalledWith(ISSUE)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`user-1`, `team-1`)
    expect(h.finalizeAttachmentUpload).toHaveBeenCalledWith(ATTACHMENT)
    expect(result).toEqual({
      id: ATTACHMENT,
      url: `/api/attachments/${ATTACHMENT}`,
      filename: `shot.png`,
      contentType: `image/png`,
      sizeBytes: 33_000,
      markdown: `![shot.png](/api/attachments/${ATTACHMENT})`,
      width: 800,
      height: 600,
      durationMs: null,
    })
  })

  it(`reports the finalized numbers, not the pre-finalize row`, async () => {
    h.rows.current = [attachmentRow({ sizeBytes: 0 })]
    h.finalizeAttachmentUpload.mockResolvedValue(
      attachmentRow({ sizeBytes: 22_000 })
    )
    const result = await finalizeSignedAttachmentUpload({
      attachmentId: ATTACHMENT,
      userId: `user-1`,
      access: FULL_ACCESS,
    })
    expect(result.sizeBytes).toBe(22_000)
  })

  it(`tells the agent to run the curl line when nothing has landed yet`, async () => {
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: FULL_ACCESS,
      })
    ).rejects.toThrow(/No upload has landed/)
    expect(h.finalizeAttachmentUpload).not.toHaveBeenCalled()
  })

  it(`refuses a row uploaded by someone else`, async () => {
    h.rows.current = [attachmentRow({ uploaderId: `user-2` })]
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: FULL_ACCESS,
      })
    ).rejects.toThrow(/not uploaded by you/)
    expect(h.finalizeAttachmentUpload).not.toHaveBeenCalled()
  })

  it(`reruns the mint's access checks: an OAuth grant confined to another board cannot finalize`, async () => {
    h.rows.current = [attachmentRow()]
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: boardGrant(`board-other`, `team-1`),
      })
    ).rejects.toThrow()
    expect(h.assertTeamMember).not.toHaveBeenCalled()
    expect(h.finalizeAttachmentUpload).not.toHaveBeenCalled()
    // The grant that covers the row's board passes.
    h.finalizeAttachmentUpload.mockResolvedValue(attachmentRow())
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: boardGrant(`board-1`, `team-1`),
      })
    ).resolves.toMatchObject({ id: ATTACHMENT })
  })

  it(`refuses an uploader who has left the team, or whose issue no longer resolves`, async () => {
    h.rows.current = [attachmentRow()]
    h.assertTeamMember.mockRejectedValueOnce(new Error(`Not a team member`))
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: FULL_ACCESS,
      })
    ).rejects.toThrow(/Not a team member/)
    h.getIssueTeamContext.mockRejectedValueOnce(new Error(`Issue not found`))
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: FULL_ACCESS,
      })
    ).rejects.toThrow(/Issue not found/)
    expect(h.finalizeAttachmentUpload).not.toHaveBeenCalled()
  })

  it(`surfaces a finalize refusal (object missing) unchanged`, async () => {
    h.rows.current = [attachmentRow()]
    h.finalizeAttachmentUpload.mockRejectedValue(
      new Error(`object not in storage yet`)
    )
    await expect(
      finalizeSignedAttachmentUpload({
        attachmentId: ATTACHMENT,
        userId: `user-1`,
        access: FULL_ACCESS,
      })
    ).rejects.toThrow(/not in storage/)
  })
})

describe(`finalizedAttachmentResult markdown`, () => {
  it(`embeds images, links inline media, leaves other files bare`, () => {
    const url = `/api/attachments/${ATTACHMENT}`
    expect(
      finalizedAttachmentResult(attachmentRow({ contentType: `image/webp` }))
        .markdown
    ).toBe(`![shot.png](${url})`)
    expect(
      finalizedAttachmentResult(
        attachmentRow({ contentType: `video/mp4`, filename: `clip.mp4` })
      ).markdown
    ).toBe(`[clip.mp4](${url})`)
    const pdf = finalizedAttachmentResult(
      attachmentRow({ contentType: `application/pdf`, filename: `a.pdf` })
    )
    expect(`markdown` in pdf).toBe(false)
  })
})
