import { describe, expect, it } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import {
  ISSUE_SEARCH_SCAN_CAP,
  issueSearchFtsQuery,
  issueSearchIdentifierExactSql,
  issueSearchMatchIds,
  issueSearchRankSql,
} from "./issue-search-sql"

// EXP-892: the ONE server predicate behind tRPC `issues.search` and MCP
// `exponential_issues_list({search})`. Rendered through the pg dialect so
// the shape (three unioned branches, tokenized ILIKE, scoping) is locked
// without a database.
function render(
  query: string,
  scope: Parameters<typeof issueSearchMatchIds>[1],
  options?: Parameters<typeof issueSearchMatchIds>[2]
) {
  return new PgDialect().sqlToQuery(issueSearchMatchIds(query, scope, options))
}

describe(`issueSearchMatchIds`, () => {
  it(`unions the issue FTS, comment FTS and tokenized ILIKE branches`, () => {
    const { sql, params } = render(`fix #87`, { teamId: `11111111-1111-4111-8111-111111111111` })
    expect(sql).toContain(`to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, ''))`)
    expect(sql).toContain(`to_tsvector('english', c.body)`)
    expect((sql.match(/union/g) ?? []).length).toBe(2)
    // Every token must match title OR identifier (and semantics), `#` gone.
    expect(sql).toContain(`(i.title ilike $`)
    expect(params).toContain(`%fix%`)
    expect(params).toContain(`%87%`)
    expect(params).not.toContain(`%#87%`)
    // The FTS text is the normalized query.
    expect(params).toContain(`fix 87`)
  })

  it(`scopes a team through the trigger-mirrored trash and archive columns`, () => {
    const { sql } = render(`login`, { teamId: `11111111-1111-4111-8111-111111111111` })
    expect(sql).toContain(`i.team_id = $`)
    expect(sql).toContain(`i.board_deleted_at is null and i.board_archived_at is null`)
    expect(sql).toContain(`c.team_id = $`)
    expect(sql).toContain(`c.board_deleted_at is null and c.board_archived_at is null`)
  })

  it(`scopes an explicit board list on both tables`, () => {
    const { sql, params } = render(`login`, {
      boardIds: [`22222222-2222-4222-8222-222222222222`, `33333333-3333-4333-8333-333333333333`],
    })
    expect(sql).toContain(`i.board_id in ($`)
    expect(sql).toContain(`c.board_id in ($`)
    expect(params.filter((p) => p === `22222222-2222-4222-8222-222222222222`).length).toBe(3)
  })

  it(`matches nothing for an empty board list or an empty query`, () => {
    expect(render(`login`, { boardIds: [] }).sql).toContain(`false`)
    expect(render(`#`, { teamId: `11111111-1111-4111-8111-111111111111` }).sql).toContain(`where false`)
  })

  it(`escapes LIKE wildcards per token`, () => {
    const { params } = render(`100%_done`, { teamId: `11111111-1111-4111-8111-111111111111` })
    expect(params).toContain(`%100\\%\\_done%`)
  })

  // EXP-892: MCP's search spans every granted board, so it passes a cap.
  it(`caps every branch when a limit is given, and only then`, () => {
    const scope = { teamId: `11111111-1111-4111-8111-111111111111` }
    const { sql, params } = render(`login`, scope, { limit: ISSUE_SEARCH_SCAN_CAP })
    // One LIMIT per branch — a bare `limit` after a union binds to the union
    // and would let the ILIKE scan run to completion first.
    expect((sql.match(/limit \$/g) ?? []).length).toBe(3)
    expect(params.filter((p) => p === ISSUE_SEARCH_SCAN_CAP).length).toBe(3)
    // Still ONE `select id` subquery for `where issues.id in (...)`.
    expect(sql.trim().startsWith(`select m.id from (`)).toBe(true)
    expect((sql.match(/union/g) ?? []).length).toBe(2)
    // No ORDER BY: sorting would compute the whole match set the cap avoids.
    expect(sql).not.toContain(`order by`)
    // Uncapped callers (tRPC `issues.search`, one team) are untouched.
    expect(render(`login`, scope).sql).not.toContain(`limit`)
  })

  it(`ranks on the same tsvector expression the index defines`, () => {
    const { sql, params } = new PgDialect().sqlToQuery(issueSearchRankSql(`#Login`))
    expect(sql).toContain(`ts_rank(to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, '')), websearch_to_tsquery('english', $1))`)
    expect(params).toEqual([`login`])
    expect(issueSearchFtsQuery(`  #Login  `)).toBe(`login`)
  })
})

// EXP-892: a row matched only by the identifier ILIKE branch has ts_rank 0,
// so `issues.search` orders on this AHEAD of the rank or `limit` cuts the
// exact issue the person typed.
describe(`issueSearchIdentifierExactSql`, () => {
  const render = (query: string) =>
    new PgDialect().sqlToQuery(issueSearchIdentifierExactSql(query))

  it(`compares the normalized query, not a pattern`, () => {
    const { sql, params } = render(`#EXP-42`)
    expect(sql).toBe(`case when i.identifier ilike $1 then 1 else 0 end`)
    // Lowercased and `#`-shorn (ILIKE handles the case), and bare: no `%`
    // wrapper, so this is an exact match and not a prefix one.
    expect(params).toEqual([`exp-42`])
  })

  it(`escapes LIKE metacharacters so they stay literal`, () => {
    expect(render(`a%b_c\\d`).params).toEqual([`a\\%b\\_c\\\\d`])
  })
})
