import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

// EXP-892: `issues.search` ranks with ts_rank, which is 0 for a row that
// matched only through the identifier ILIKE branch — so an exact `EXP-42`
// could be cut by `limit` behind rows that merely mention the words. The
// exact-identifier term must sort AHEAD of the rank. The ordering is only
// observable against a live Postgres, so this locks the source instead; the
// term itself (an escaped, wildcard-free ILIKE) is covered by
// issue-search-sql.test.ts.
const SOURCE = readFileSync(join(import.meta.dirname, `issues.ts`), `utf8`)

describe(`issues.search ordering`, () => {
  it(`sorts an exact identifier hit above ts_rank`, () => {
    const exact = SOURCE.indexOf(`issueSearchIdentifierExactSql(input.query)`)
    const rank = SOURCE.indexOf(`issueSearchRankSql(input.query)`)
    expect(exact).toBeGreaterThan(-1)
    expect(rank).toBeGreaterThan(-1)
    expect(exact).toBeLessThan(rank)
    expect(SOURCE.slice(exact, rank)).toContain(`desc`)
  })

  // EXP-922: the server's three ordering keys must be the client engine's
  // three, in the same order (exact identifier, then undone, then relevance)
  // — otherwise the hits spliced in behind the local ranking arrive in a
  // different order than the rows above them.
  it(`sorts undone issues above done ones, between the exact hit and ts_rank`, () => {
    const exact = SOURCE.indexOf(`issueSearchIdentifierExactSql(input.query)`)
    const open = SOURCE.indexOf(`issueSearchOpenSql()`)
    const rank = SOURCE.indexOf(`issueSearchRankSql(input.query)`)
    expect(open).toBeGreaterThan(-1)
    expect(exact).toBeLessThan(open)
    expect(open).toBeLessThan(rank)
    expect(SOURCE.slice(open, rank)).toContain(`desc`)
  })

  it(`keeps the wire contract: the same seven selected columns`, () => {
    // Old clients call this procedure; the row shape may not drift.
    for (const field of [
      `id: row.id as string`,
      `identifier: row.identifier as string`,
      `title: row.title as string`,
      `boardId: row.boardId as string`,
      `status: row.status as string`,
      `statusId: (row.statusId as string | null) ?? null`,
      `priority: row.priority as string`,
    ]) {
      expect(SOURCE).toContain(field)
    }
  })
})
