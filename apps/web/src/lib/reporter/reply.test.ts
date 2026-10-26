import { beforeEach, describe, expect, it, vi } from "vitest"

// NEW-19: the ONE reporter-reply write, shared by /api/support/reply (the
// magic link) and the signed-in reporter's MCP `comments create`.

const h = vi.hoisted(() => {
  const inserted: Array<Record<string, unknown>> = []
  const updates: Array<{ table: unknown; values: Record<string, unknown> }> = []
  const selectRows: Array<Array<unknown>> = []
  const tx = {
    insert: vi.fn(() => ({
      values: (values: Record<string, unknown>) => {
        inserted.push(values)
        return {
          returning: async () => [
            { id: `comment-1`, createdAt: new Date(`2026-10-26T12:00:00Z`) },
          ],
        }
      },
    })),
    select: vi.fn(() => {
      const b: Record<string, unknown> = {}
      b.from = () => b
      b.where = () => b
      b.limit = async () => selectRows.shift() ?? []
      return b
    }),
    update: vi.fn((table: unknown) => ({
      set: (values: Record<string, unknown>) => {
        updates.push({ table, values })
        return { where: async () => undefined }
      },
    })),
  }
  return {
    inserted,
    updates,
    selectRows,
    tx,
    recordIssueEvent: vi.fn(),
    notify: vi.fn(),
  }
})

vi.mock(`@/db/connection`, () => ({
  db: { transaction: async (fn: (tx: unknown) => unknown) => fn(h.tx) },
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn(async () => 1) }))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetReporterReplyNotify: h.notify,
}))

import { issues, widgetSubmissions } from "@/db/schema"
import { postReporterReply, validateReporterReplyText } from "./reply"

const issue = (over: Record<string, unknown> = {}) =>
  ({
    id: `issue-1`,
    teamId: `team-1`,
    boardId: `board-1`,
    status: `in_progress`,
    statusId: `status-ip`,
    ...over,
  }) as never

beforeEach(() => {
  h.inserted.length = 0
  h.updates.length = 0
  h.selectRows.length = 0
  vi.clearAllMocks()
})

describe(`validateReporterReplyText`, () => {
  it(`trims and bounds the words`, () => {
    expect(validateReporterReplyText(`  hi  `)).toEqual({ ok: true, text: `hi` })
    expect(validateReporterReplyText(`   `)).toEqual({
      ok: false,
      error: `Message is empty`,
    })
    expect(validateReporterReplyText(42)).toEqual({
      ok: false,
      error: `Message is empty`,
    })
    expect(validateReporterReplyText(`x`.repeat(10_001))).toEqual({
      ok: false,
      error: `Message is too long`,
    })
  })
})

describe(`postReporterReply`, () => {
  it(`writes a reporter comment, stamps the read receipt and notifies`, async () => {
    const result = await postReporterReply(
      { issue: issue(), submission: { id: `sub-1` } as never },
      `*not* bold`
    )
    expect(result).toEqual({
      ok: true,
      message: { id: `comment-1`, createdAt: new Date(`2026-10-26T12:00:00Z`) },
    })
    expect(h.inserted).toEqual([
      {
        issueId: `issue-1`,
        teamId: `team-1`,
        boardId: `board-1`,
        authorId: null,
        parentId: null,
        source: `reporter`,
        audience: `reporter`,
        body: `\\*not\\* bold`,
      },
    ])
    expect(h.updates.map((u) => u.table)).toEqual([widgetSubmissions])
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
    expect(h.notify).toHaveBeenCalledWith({
      issueId: `issue-1`,
      commentId: `comment-1`,
    })
  })

  it(`reopens a completed issue to backlog with one status_changed event`, async () => {
    h.selectRows.push([{ id: `status-backlog`, name: `Backlog` }], [{ name: `Done` }])
    await postReporterReply(
      {
        issue: issue({ id: `issue-2`, status: `done`, statusId: `status-done` }),
        submission: { id: `sub-2` } as never,
      },
      `It came back`
    )
    expect(h.updates[0]).toEqual({
      table: issues,
      values: { status: `backlog`, statusId: `status-backlog`, completedAt: null },
    })
    expect(h.recordIssueEvent).toHaveBeenCalledWith(h.tx, {
      issueId: `issue-2`,
      teamId: `team-1`,
      actorUserId: null,
      type: `status_changed`,
      payload: {
        fromStatusId: `status-done`,
        toStatusId: `status-backlog`,
        fromName: `Done`,
        toName: `Backlog`,
      },
    })
  })

  it(`keeps the per-issue reply bucket`, async () => {
    const resolved = { issue: issue({ id: `issue-3` }), submission: { id: `s` } as never }
    for (let i = 0; i < 5; i += 1) {
      expect((await postReporterReply(resolved, `r${i}`)).ok).toBe(true)
    }
    const limited = await postReporterReply(resolved, `one too many`)
    expect(limited.ok).toBe(false)
    expect(h.inserted).toHaveLength(5)
  })
})
