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
    orderBy: () => b,
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
import { findIssueByReporterToken, findIssueForReporter } from "./service"

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

// NEW-19 (FEED-144): the signed-in reporter's twin of the magic link — their
// VERIFIED account email must be the submission's reporter email.
describe(`findIssueForReporter`, () => {
  const verified = { email: `Emma@Example.com`, emailVerified: true }
  const where = () =>
    new PgDialect().sqlToQuery(h.where as SQL)

  it(`never queries for an unverified or missing email`, async () => {
    await expect(
      findIssueForReporter({ ...verified, emailVerified: false }, `FEED-1`)
    ).resolves.toBeNull()
    await expect(
      findIssueForReporter({ email: null, emailVerified: true }, `FEED-1`)
    ).resolves.toBeNull()
    expect(h.where).toBeNull()
  })

  it(`matches the identifier (upper-cased) and the email case-insensitively on a visible board`, async () => {
    await findIssueForReporter(verified, `feed-144`)
    const { sql, params } = where()
    expect(sql).toContain(`"issues"."identifier" = $`)
    expect(sql).toContain(`lower("widget_submissions"."reporter_email") = $`)
    expect(sql.toLowerCase()).toContain(`"boards"."deleted_at" is null`)
    expect(sql.toLowerCase()).toContain(`"boards"."archived_at" is null`)
    expect(params).toContain(`FEED-144`)
    expect(params).toContain(`emma@example.com`)
  })

  it(`matches a UUID by id`, async () => {
    const id = `11111111-1111-4111-8111-111111111111`
    await findIssueForReporter(verified, id)
    const { sql, params } = where()
    expect(sql).toContain(`"issues"."id" = $`)
    expect(params).toContain(id)
  })

  it(`returns the row with its status category`, async () => {
    h.rows = [
      {
        issue: { id: `i1` },
        submission: { id: `s1` },
        teamName: `Exponential`,
        statusCategory: `completed`,
      },
    ]
    await expect(findIssueForReporter(verified, `FEED-1`)).resolves.toEqual({
      issue: { id: `i1` },
      submission: { id: `s1` },
      teamName: `Exponential`,
      statusCategory: `completed`,
    })
  })
})
