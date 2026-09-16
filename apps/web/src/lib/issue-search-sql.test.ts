import { describe, expect, it } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import {
  issueSearchFtsQuery,
  issueSearchMatchIds,
  issueSearchRankSql,
} from "./issue-search-sql"

// EXP-892: the ONE server predicate behind tRPC `issues.search` and MCP
// `exponential_issues_list({search})`. Rendered through the pg dialect so
// the shape (three unioned branches, tokenized ILIKE, scoping) is locked
// without a database.
function render(query: string, scope: Parameters<typeof issueSearchMatchIds>[1]) {
  return new PgDialect().sqlToQuery(issueSearchMatchIds(query, scope))
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

  it(`ranks on the same tsvector expression the index defines`, () => {
    const { sql, params } = new PgDialect().sqlToQuery(issueSearchRankSql(`#Login`))
    expect(sql).toContain(`ts_rank(to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, '')), websearch_to_tsquery('english', $1))`)
    expect(params).toEqual([`login`])
    expect(issueSearchFtsQuery(`  #Login  `)).toBe(`login`)
  })
})
