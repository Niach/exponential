import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-979: the handler behind `exponential_attachments_list` and
// `exponential_issues_get`'s `attachmentCount`. Access checks live in the
// registry (tools.test.ts); this file proves the QUERY: scoped to the issue,
// newest first, paged, `total` unpaged, `commentId` only when set.
const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const calls: Array<{
    where: string
    orderBy: unknown[]
    limit?: number
    offset?: number
  }> = []
  return { selectQueue, calls }
})

vi.mock(`@/db/connection`, () => {
  const dialect = new PgDialect()
  type Chain = Promise<unknown[]> & Record<string, (...args: unknown[]) => unknown>
  function chain(result: unknown[]): Chain {
    const call: (typeof h.calls)[number] = { where: ``, orderBy: [] }
    h.calls.push(call)
    const p = Promise.resolve(result) as Chain
    p.from = () => p
    p.where = (arg?: unknown) => {
      const q = dialect.sqlToQuery(arg as never)
      call.where = `${q.sql} ${JSON.stringify(q.params)}`
      return p
    }
    p.orderBy = (...args: unknown[]) => {
      call.orderBy = args
      return p
    }
    p.limit = (n?: unknown) => {
      call.limit = n as number
      return p
    }
    p.offset = (n?: unknown) => {
      call.offset = n as number
      return p
    }
    return p
  }
  return {
    db: {
      select: vi.fn(() => chain(h.selectQueue.shift() ?? [])),
    },
  }
})

import {
  countIssueAttachments,
  listIssueAttachments,
} from "./attachments-list"

const ISSUE = `11111111-1111-4111-8111-111111111111`
const COMMENT = `22222222-2222-4222-8222-222222222222`

beforeEach(() => {
  h.selectQueue.length = 0
  h.calls.length = 0
})

describe(`listIssueAttachments (EXP-979)`, () => {
  it(`lists the issue's rows newest first with the unpaged total`, async () => {
    h.selectQueue.push(
      [
        {
          id: `a2`,
          filename: `research.md`,
          contentType: `text/markdown`,
          sizeBytes: 1234,
          createdAt: new Date(`2026-09-19T10:00:00.000Z`),
          commentId: COMMENT,
        },
        {
          id: `a1`,
          filename: `shot.png`,
          contentType: `image/png`,
          sizeBytes: 99,
          createdAt: new Date(`2026-09-18T10:00:00.000Z`),
          commentId: null,
        },
      ],
      [{ total: 7 }]
    )
    const result = await listIssueAttachments({
      issueId: ISSUE,
      limit: 2,
      offset: 3,
    })
    expect(result).toEqual({
      attachments: [
        {
          id: `a2`,
          filename: `research.md`,
          contentType: `text/markdown`,
          sizeBytes: 1234,
          createdAt: `2026-09-19T10:00:00.000Z`,
          commentId: COMMENT,
        },
        {
          id: `a1`,
          filename: `shot.png`,
          contentType: `image/png`,
          sizeBytes: 99,
          createdAt: `2026-09-18T10:00:00.000Z`,
        },
      ],
      total: 7,
    })
    // No signed URL, storage key or poster leaks into the list.
    for (const entry of result.attachments) {
      expect(Object.keys(entry)).not.toContain(`downloadUrl`)
      expect(Object.keys(entry)).not.toContain(`storageKey`)
    }
    const [list, total] = h.calls
    expect(list.where).toBe(`"attachments"."issue_id" = $1 ["${ISSUE}"]`)
    expect(list.orderBy).toHaveLength(2)
    expect(list.limit).toBe(2)
    expect(list.offset).toBe(3)
    // The count is scoped the same way and never paged.
    expect(total.where).toBe(list.where)
    expect(total.limit).toBeUndefined()
    expect(total.offset).toBeUndefined()
  })

  it(`returns an empty page and total 0 for an issue without files`, async () => {
    h.selectQueue.push([], [])
    await expect(
      listIssueAttachments({ issueId: ISSUE, limit: 50, offset: 0 })
    ).resolves.toEqual({ attachments: [], total: 0 })
  })
})

describe(`countIssueAttachments (EXP-979)`, () => {
  it(`reads the issue-scoped count`, async () => {
    h.selectQueue.push([{ total: 3 }])
    await expect(countIssueAttachments(ISSUE)).resolves.toBe(3)
    expect(h.calls[0].where).toBe(
      `"attachments"."issue_id" = $1 ["${ISSUE}"]`
    )
  })

  it(`reads 0 when the count row is missing`, async () => {
    h.selectQueue.push([])
    await expect(countIssueAttachments(ISSUE)).resolves.toBe(0)
  })
})
