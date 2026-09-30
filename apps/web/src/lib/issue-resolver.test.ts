import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// FEED-66: a run launched on FEED-50 kept calling it FEED-50 after a
// cross-board move renumbered it to EXP-1147 — `issues_get` had answered,
// `pr_open` minutes later said "Issue not found". The retired identifier
// resolves through the move's `board_moved` event, under the SAME scoping as
// the direct lookup; a never-moved miss is still a miss.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  wheres: [] as unknown[],
  getUserTeamIds: vi.fn(async () => [`ws-1`]),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: vi.fn(() => {
      const rows = h.selectQueue.shift() ?? []
      const builder: Record<string, unknown> = {}
      for (const method of [`from`, `innerJoin`, `orderBy`, `limit`]) {
        builder[method] = () => builder
      }
      builder.where = (cond: unknown) => {
        h.wheres.push(cond)
        return builder
      }
      ;(builder as { then: unknown }).then = (
        resolve: (v: unknown) => unknown,
        reject: (e: unknown) => unknown
      ) => Promise.resolve(rows).then(resolve, reject)
      return builder
    }),
  },
}))
vi.mock(`@/lib/team-membership`, () => ({
  getUserTeamIds: h.getUserTeamIds,
}))

import { resolveIssueReference, retiredIdentifiers } from "@/lib/issue-resolver"

const ISSUE = `3f8b5bc1-ddf8-496c-ab2a-887a848cfc1d`

function renderedWhere(index: number): { sql: string; params: unknown[] } {
  return new PgDialect().sqlToQuery(h.wheres[index] as never)
}

beforeEach(() => {
  h.selectQueue.length = 0
  h.wheres.length = 0
  vi.clearAllMocks()
})

describe(`resolveIssueReference`, () => {
  it(`passes a UUID through without a lookup`, async () => {
    await expect(resolveIssueReference(`user-1`, ISSUE)).resolves.toBe(ISSUE)
    expect(h.wheres).toHaveLength(0)
  })

  it(`resolves a current identifier in one query, uppercased`, async () => {
    h.selectQueue.push([{ id: ISSUE }])
    await expect(resolveIssueReference(`user-1`, `exp-1147`)).resolves.toBe(ISSUE)
    expect(h.wheres).toHaveLength(1)
    const { sql, params } = renderedWhere(0)
    expect(sql).toContain(`"identifier" =`)
    expect(params).toContain(`EXP-1147`)
  })

  it(`resolves a RETIRED identifier through the move event, same scoping (FEED-66)`, async () => {
    h.selectQueue.push([])
    h.selectQueue.push([{ id: ISSUE }])
    await expect(
      resolveIssueReference(`user-1`, `FEED-50`, { grantedBoardIds: [`board-exp`] })
    ).resolves.toBe(ISSUE)
    expect(h.wheres).toHaveLength(2)
    const { sql, params } = renderedWhere(1)
    expect(sql).toContain(`'fromIdentifier' =`)
    expect(params).toContain(`FEED-50`)
    expect(params).toContain(`board_moved`)
    // The issue's CURRENT board must be live and granted, like the direct hit.
    expect(sql).toContain(`"board_deleted_at" is null`)
    expect(sql).toContain(`"board_archived_at" is null`)
    expect(sql).toContain(`"board_id" in`)
    expect(params).toContain(`board-exp`)
    expect(params).toContain(`ws-1`)
  })

  it(`still misses an identifier nothing ever carried`, async () => {
    h.selectQueue.push([])
    h.selectQueue.push([])
    await expect(resolveIssueReference(`user-1`, `FEED-999`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
      message: `Issue not found: FEED-999`,
    })
  })

  it(`misses everything for a member of no team, without querying`, async () => {
    h.getUserTeamIds.mockResolvedValueOnce([])
    await expect(resolveIssueReference(`user-1`, `FEED-50`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    expect(h.wheres).toHaveLength(0)
  })
})

describe(`retiredIdentifiers`, () => {
  it(`lists the identifiers an issue carried before its moves, newest first, deduplicated`, async () => {
    h.selectQueue.push([
      { identifier: `EXP-1147` },
      { identifier: `FEED-50` },
      { identifier: ` FEED-50 ` },
      { identifier: null },
      { identifier: `` },
    ])
    await expect(retiredIdentifiers(ISSUE)).resolves.toEqual([`EXP-1147`, `FEED-50`])
    const { sql, params } = renderedWhere(0)
    expect(sql).toContain(`"issue_id" =`)
    expect(params).toEqual([ISSUE, `board_moved`])
  })

  it(`is empty for a never-moved issue`, async () => {
    await expect(retiredIdentifiers(ISSUE)).resolves.toEqual([])
  })
})
