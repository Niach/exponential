import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import type { SQL } from "drizzle-orm"

// The reporter token must stop resolving once the issue's board is trashed
// or archived: no read, reply or reopen through a hidden board.

const h = vi.hoisted(() => ({
  joins: [] as unknown[],
  where: null as unknown,
  rows: [] as unknown[],
}))

vi.mock(`@/db/connection`, () => {
  const b = {
    from: () => b,
    innerJoin: (table: unknown) => {
      h.joins.push(table)
      return b
    },
    leftJoin: () => b,
    where: (condition: unknown) => {
      h.where = condition
      return b
    },
    limit: async () => h.rows,
  }
  return { db: { select: vi.fn(() => b) } }
})

vi.mock(`@/lib/reporter/token`, () => ({
  verifyReporterToken: (token: string) =>
    token === `good` ? `11111111-1111-4111-8111-111111111111` : null,
}))

import { boards } from "@/db/schema"
import { findIssueByReporterToken } from "./service"

beforeEach(() => {
  h.joins.length = 0
  h.where = null
  h.rows = []
})

describe(`findIssueByReporterToken`, () => {
  it(`requires a visible board (not trashed, not archived)`, async () => {
    await findIssueByReporterToken(`good`)

    expect(h.joins).toContain(boards)
    const sql = new PgDialect()
      .sqlToQuery(h.where as SQL)
      .sql.toLowerCase()
    expect(sql).toContain(`"boards"."deleted_at" is null`)
    expect(sql).toContain(`"boards"."archived_at" is null`)
  })

  it(`answers null when the hidden board filters the row out`, async () => {
    await expect(findIssueByReporterToken(`good`)).resolves.toBeNull()
  })

  it(`rejects a bad token before any query`, async () => {
    await expect(findIssueByReporterToken(`bad`)).resolves.toBeNull()
    expect(h.where).toBeNull()
  })
})
