import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-search.json"
import {
  issueSearchTokens,
  mergeIssueSearchHits,
  normalizeIssueSearchQuery,
  rankIssueSearch,
  type IssueSearchRow,
} from "./issue-search"

// EXP-892: the issue-search engine, locked ×4 (Android IssueSearchTest, iOS
// IssueSearchTests, desktop domain::issue_search) against the ONE contract
// fixture — same cases, same test names.
interface FixtureCase {
  name: string
  rows: IssueSearchRow[]
  query: string
  exclude?: string[]
  limit?: number
  serverHits?: Array<{ id: string; identifier: string; title: string }>
  allowUnsynced?: boolean
  expected: string[]
}

const cases = fixture as FixtureCase[]

describe(`issue search (contract fixture)`, () => {
  for (const c of cases) {
    it(c.name, () => {
      const local = rankIssueSearch(c.rows, c.query, {
        limit: c.limit,
        exclude: c.exclude,
      })
      const byId = new Map(c.rows.map((row) => [row.id, row]))
      const merged = c.serverHits
        ? mergeIssueSearchHits(local, c.serverHits, {
            limit: c.limit,
            exclude: c.exclude,
            resolve: (hit) =>
              byId.get(hit.id) ??
              (c.allowUnsynced
                ? { id: hit.id, identifier: hit.identifier, title: hit.title }
                : null),
          })
        : local
      expect(merged.map((row) => row.id)).toEqual(c.expected)
    })
  }
})

describe(`issue search helpers`, () => {
  it(`normalizes a query`, () => {
    expect(normalizeIssueSearchQuery(`  #EXP-87  `)).toBe(`exp-87`)
    expect(normalizeIssueSearchQuery(`#`)).toBe(``)
  })

  it(`tokenizes on whitespace and drops per-token hashes`, () => {
    expect(issueSearchTokens(`Fix  #87 login`)).toEqual([`fix`, `87`, `login`])
    expect(issueSearchTokens(`#`)).toEqual([])
  })

  it(`is stable for equal keys`, () => {
    const rows = [
      { id: `a`, identifier: `X-1`, title: `same` },
      { id: `b`, identifier: `X-1`, title: `same` },
    ]
    expect(rankIssueSearch(rows, `same`).map((r) => r.id)).toEqual([`a`, `b`])
  })
})
