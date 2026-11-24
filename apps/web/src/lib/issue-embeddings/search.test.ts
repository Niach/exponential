// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import type { SQL } from "drizzle-orm"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  ISSUE_SEARCH_EMBED_BUDGET_MS,
  ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS,
  ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE,
  ISSUE_SEARCH_SEMANTIC_MAX_HITS,
  ISSUE_SEARCH_SIMILAR_MAX_HITS,
  ISSUE_SEARCH_SIMILAR_MAX_DISTANCE,
  appendSemanticHits,
  issueSearchIdsSql,
  issueSearchSemanticText,
  resetIssueSearchVectors,
  searchIssues,
  type IssueSearchBackend,
  type IssueSearchHitRow,
} from "./search"

// #NEW-135: the order, the limit, the cutoff and the budget of the one
// server search, with the database and the model faked. The real ones run in
// issue-search-semantic.real-db.test.ts.

const dialect = new PgDialect()
const render = (query: SQL | undefined) => (query ? dialect.sqlToQuery(query) : { sql: ``, params: [] })

const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, `0`)}`
const row = (n: number): IssueSearchHitRow => ({
  id: uuid(n),
  identifier: `EXP-${n}`,
  title: `Issue ${n}`,
  boardId: uuid(900),
  status: `backlog`,
  statusId: null,
  priority: `none`,
})
const scope = { teamId: uuid(800) }
const VECTOR = new Float32Array([1, 0, 0])

interface Fake {
  available?: boolean
  lexical?: number[]
  /** Semantic neighbours, nearest first. */
  nearest?: number[]
  /** The issue `similar:` resolves to. */
  source?: number | null
  embed?: (text: string) => Promise<Float32Array | null>
}

function fake(options: Fake = {}) {
  const calls = {
    lexical: [] as Array<{ query: string; limit: number }>,
    embed: [] as string[],
    source: [] as string[],
    nearest: [] as Array<{ vector: ArrayLike<number>; k: number; maxDistance: number; filter?: SQL }>,
  }
  const backend: IssueSearchBackend = {
    available: async () => options.available ?? true,
    lexical: async (query, _scope, limit) => {
      calls.lexical.push({ query, limit })
      return (options.lexical ?? []).slice(0, limit).map(row)
    },
    embed: async (text) => {
      calls.embed.push(text)
      return options.embed ? options.embed(text) : VECTOR
    },
    source: async (identifier) => {
      calls.source.push(identifier)
      return options.source == null ? null : { issueId: uuid(options.source), vector: [0, 1, 0] }
    },
    nearest: async (vector, _scope, nearestOptions) => {
      calls.nearest.push({ vector, ...nearestOptions })
      return (options.nearest ?? []).slice(0, nearestOptions.k).map((n, index) => ({ issueId: uuid(n), distance: 0.1 + index / 100 }))
    },
    // Any order: the search must restore the neighbours' own.
    rows: async (ids) => [...ids].reverse().map((id) => row(Number(id.slice(-12)))),
  }
  return { backend, calls }
}

const ids = (rows: IssueSearchHitRow[]) => rows.map((hit) => Number(hit.id.slice(-12)))

beforeEach(() => resetIssueSearchVectors())

describe(`issueSearchSemanticText (what the model reads)`, () => {
  it(`keeps the plain words in the person's spelling, drops every prefixed token and hashes`, () => {
    expect(issueSearchSemanticText(`  Anmeldung in Safari  by:lukas IS:open status:review created:7d updated:today #kaputt `)).toBe(
      `Anmeldung in Safari kaputt`
    )
    expect(issueSearchSemanticText(`similar:NEW-12 Login`)).toBe(`Login`)
  })

  it(`is null without words, for under three characters, and for one bare identifier or number`, () => {
    expect(issueSearchSemanticText(``)).toBeNull()
    expect(issueSearchSemanticText(`is:open by:lukas`)).toBeNull()
    expect(issueSearchSemanticText(`ab`)).toBeNull()
    expect(issueSearchSemanticText(`#87`)).toBeNull()
    expect(issueSearchSemanticText(`1234`)).toBeNull()
    expect(issueSearchSemanticText(`EXP-42`)).toBeNull()
    expect(issueSearchSemanticText(`new-135`)).toBeNull()
    expect(issueSearchSemanticText(`EXP-42 crash`)).toBe(`EXP-42 crash`)
    expect(issueSearchSemanticText(`abc`)).toBe(`abc`)
  })
})

describe(`appendSemanticHits`, () => {
  it(`keeps the lexical order, appends unseen neighbours in theirs, up to the cap and the limit`, () => {
    const lexical = [row(1), row(2)]
    const semantic = [row(2), row(9), row(8), row(1), row(7), row(6), row(5), row(4)]
    expect(appendSemanticHits(lexical, semantic, 20).map((hit) => hit.identifier)).toEqual(
      [`EXP-1`, `EXP-2`, `EXP-9`, `EXP-8`, `EXP-7`, `EXP-6`, `EXP-5`].slice(0, 2 + ISSUE_SEARCH_SEMANTIC_MAX_HITS)
    )
    expect(ISSUE_SEARCH_SEMANTIC_MAX_HITS).toBeLessThanOrEqual(5)
    expect(appendSemanticHits(lexical, semantic, 3).map((hit) => hit.identifier)).toEqual([`EXP-1`, `EXP-2`, `EXP-9`])
    expect(appendSemanticHits(lexical, semantic, 2)).toEqual(lexical)
    expect(appendSemanticHits(lexical, semantic, 1)).toEqual([row(1)])
  })
})

describe(`searchIssues: a worded query`, () => {
  it(`lists the lexical hits first, then the neighbours the lexical branches missed, nearest first`, async () => {
    const { backend, calls } = fake({ lexical: [3, 1], nearest: [7, 5] })
    expect(ids(await searchIssues(`Anmeldung kaputt is:open`, scope, 20, { backend }))).toEqual([3, 1, 7, 5])
    expect(calls.lexical).toEqual([{ query: `Anmeldung kaputt is:open`, limit: 20 }])
    expect(calls.embed).toEqual([`Anmeldung kaputt`])
    expect(calls.nearest).toHaveLength(1)
    const nearest = calls.nearest[0]!
    expect(nearest.vector).toBe(VECTOR)
    expect(nearest.k).toBe(ISSUE_SEARCH_SEMANTIC_MAX_HITS)
    expect(nearest.maxDistance).toBe(ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE)
    // The lexical hits are excluded IN the kNN (they cost no slot), and the
    // filter tokens narrow the neighbours too.
    const filter = render(nearest.filter)
    expect(filter.sql).toContain(`i.id not in (`)
    expect(filter.params).toEqual(expect.arrayContaining([uuid(3), uuid(1)]))
    expect(filter.sql).toContain(`i.status in (`)
  })

  it(`asks for only as many neighbours as the limit leaves, and for none when lexical fills it`, async () => {
    const some = fake({ lexical: [1, 2, 3], nearest: [7, 8, 9] })
    expect(ids(await searchIssues(`login broken`, scope, 5, { backend: some.backend }))).toEqual([1, 2, 3, 7, 8])
    expect(some.calls.nearest[0]!.k).toBe(2)

    resetIssueSearchVectors()
    const full = fake({ lexical: [1, 2, 3, 4, 5], nearest: [7] })
    expect(ids(await searchIssues(`login broken`, scope, 5, { backend: full.backend }))).toEqual([1, 2, 3, 4, 5])
    expect(full.calls.nearest).toEqual([])
  })

  it(`never appends more than the cap, whatever the kNN returns`, async () => {
    const { backend } = fake({ lexical: [1], nearest: [11, 12, 13, 14, 15, 16, 17] })
    backend.nearest = async () => [11, 12, 13, 14, 15, 16, 17].map((n) => ({ issueId: uuid(n), distance: 0.1 }))
    expect(ids(await searchIssues(`login broken`, scope, 30, { backend }))).toEqual([1, 11, 12, 13, 14, 15, 16, 17].slice(0, 1 + ISSUE_SEARCH_SEMANTIC_MAX_HITS))
  })

  it(`with no lexical hit the neighbours are the list, unfiltered by ids`, async () => {
    const { backend, calls } = fake({ lexical: [], nearest: [7, 5] })
    expect(ids(await searchIssues(`kann mich nicht einloggen`, scope, 20, { backend }))).toEqual([7, 5])
    expect(calls.nearest[0]!.filter).toBeUndefined()
  })

  it(`stays lexical for a filter-only, a too short or an identifier query: the model is never asked`, async () => {
    for (const query of [`is:open by:lukas`, `ab`, `EXP-42`, `#87`]) {
      const { backend, calls } = fake({ lexical: [1], nearest: [7] })
      expect(ids(await searchIssues(query, scope, 20, { backend }))).toEqual([1])
      expect(calls.embed).toEqual([])
      expect(calls.nearest).toEqual([])
    }
  })

  it(`stays lexical without pgvector or the model, when the embedder answers null, and when it throws`, async () => {
    const off = fake({ available: false, lexical: [1], nearest: [7] })
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend: off.backend }))).toEqual([1])
    expect(off.calls.embed).toEqual([])

    const none = fake({ lexical: [1], nearest: [7], embed: async () => null })
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend: none.backend }))).toEqual([1])
    expect(none.calls.nearest).toEqual([])

    resetIssueSearchVectors()
    const broken = fake({ lexical: [1], nearest: [7], embed: async () => Promise.reject(new Error(`worker gone`)) })
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend: broken.backend }))).toEqual([1])
  })

  it(`stays lexical when the kNN or the row read fails`, async () => {
    const log = vi.spyOn(console, `error`).mockImplementation(() => {})
    const { backend } = fake({ lexical: [1], nearest: [7] })
    backend.nearest = async () => Promise.reject(new Error(`no such table`))
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend }))).toEqual([1])
    expect(log).toHaveBeenCalledTimes(1)
    log.mockRestore()
  })

  it(`returns lexical only when the embedding misses its budget, and uses the late vector next time`, async () => {
    let release: (vector: Float32Array) => void = () => {}
    const slow = new Promise<Float32Array>((resolve) => {
      release = resolve
    })
    const { backend, calls } = fake({ lexical: [1], nearest: [7], embed: () => slow })
    const started = Date.now()
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend, budgetMs: 30 }))).toEqual([1])
    expect(Date.now() - started).toBeLessThan(1000)
    expect(calls.nearest).toEqual([])

    // The model finishes after the answer went out: the next request for the
    // same words has its vector at once, without a second inference.
    release(VECTOR)
    await slow
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend, budgetMs: 5 }))).toEqual([1, 7])
    expect(calls.embed).toEqual([`login broken`])
  })

  it(`waits longer when no word matched (nothing to hold back), never when lexical hits are waiting`, async () => {
    const late = () => new Promise<Float32Array>((resolve) => setTimeout(() => resolve(VECTOR), ISSUE_SEARCH_EMBED_BUDGET_MS + 150))
    expect(ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS).toBeGreaterThan(ISSUE_SEARCH_EMBED_BUDGET_MS + 300)
    const empty = fake({ lexical: [], nearest: [7], embed: late })
    expect(ids(await searchIssues(`cold model sentence`, scope, 20, { backend: empty.backend }))).toEqual([7])
    resetIssueSearchVectors()
    const some = fake({ lexical: [1], nearest: [7], embed: late })
    const started = Date.now()
    expect(ids(await searchIssues(`cold model sentence`, scope, 20, { backend: some.backend }))).toEqual([1])
    expect(Date.now() - started).toBeLessThan(ISSUE_SEARCH_EMBED_BUDGET_MS + 120)
  })

  it(`the budget counts from the start of the search: a slow lexical query leaves none`, async () => {
    const { backend, calls } = fake({ lexical: [1], nearest: [7], embed: () => new Promise((resolve) => setTimeout(() => resolve(VECTOR), 60)) })
    const lexical = backend.lexical
    backend.lexical = async (...args) => {
      await new Promise((resolve) => setTimeout(resolve, 40))
      return lexical(...args)
    }
    expect(ids(await searchIssues(`login broken`, scope, 20, { backend, budgetMs: 20 }))).toEqual([1])
    expect(calls.nearest).toEqual([])
  })
})

describe(`searchIssues: similar:<ID>`, () => {
  it(`lists that issue's neighbours in the kNN's order, itself excluded, lexical branches not run`, async () => {
    const { backend, calls } = fake({ lexical: [1, 2], source: 12, nearest: [7, 5, 9] })
    expect(ids(await searchIssues(`similar:#NEW-12`, scope, 20, { backend }))).toEqual([7, 5, 9])
    expect(calls.source).toEqual([`new-12`])
    expect(calls.lexical).toEqual([])
    expect(calls.embed).toEqual([])
    const nearest = calls.nearest[0]!
    expect(Array.from(nearest.vector)).toEqual([0, 1, 0])
    expect(nearest.k).toBe(ISSUE_SEARCH_SIMILAR_MAX_HITS)
    expect(nearest.maxDistance).toBe(ISSUE_SEARCH_SIMILAR_MAX_DISTANCE)
    await searchIssues(`similar:NEW-12`, scope, 3, { backend })
    expect(calls.nearest[1]!.k).toBe(3)
    const filter = render(nearest.filter)
    expect(filter.sql).toContain(`i.id <> `)
    expect(filter.params).toEqual([uuid(12)])
  })

  it(`its other tokens narrow the neighbours: filters, and words lexically`, async () => {
    const { backend, calls } = fake({ source: 12, nearest: [7] })
    await searchIssues(`similar:NEW-12 is:closed safari`, scope, 20, { backend })
    const filter = render(calls.nearest[0]!.filter)
    expect(filter.sql).toContain(`i.id <> `)
    expect(filter.sql).toContain(`i.id in (`)
    expect(filter.sql).toContain(`websearch_to_tsquery`)
    expect(filter.sql).toContain(`i.status in (`)
    expect(filter.params).toContain(`safari`)
    expect(JSON.stringify(filter.params)).not.toContain(`similar`)
  })

  it(`an unknown, foreign or not yet embedded issue, or no pgvector, is simply no hits`, async () => {
    const unknown = fake({ lexical: [1], source: null, nearest: [7] })
    expect(await searchIssues(`similar:NOPE-1 login`, scope, 20, { backend: unknown.backend })).toEqual([])
    expect(unknown.calls.nearest).toEqual([])
    expect(unknown.calls.lexical).toEqual([])

    const off = fake({ available: false, lexical: [1], source: 12, nearest: [7] })
    expect(await searchIssues(`similar:NEW-12`, scope, 20, { backend: off.backend })).toEqual([])
    expect(off.calls.source).toEqual([])
  })

  it(`a failing lookup is no hits, never an error`, async () => {
    const log = vi.spyOn(console, `error`).mockImplementation(() => {})
    const { backend } = fake({ source: 12, nearest: [7] })
    backend.source = async () => Promise.reject(new Error(`boom`))
    expect(await searchIssues(`similar:NEW-12`, scope, 20, { backend })).toEqual([])
    log.mockRestore()
  })

  it(`an empty similar: is not one: the query is its words`, async () => {
    const { backend, calls } = fake({ lexical: [1], nearest: [] })
    expect(ids(await searchIssues(`login similar: similar:#`, scope, 20, { backend }))).toEqual([1])
    expect(calls.source).toEqual([])
  })
})

describe(`issueSearchIdsSql (MCP list search)`, () => {
  const boards = { boardIds: [uuid(900)] }

  it(`a worded query: the lexical subquery unioned with the neighbours it missed`, async () => {
    const { backend, calls } = fake({ nearest: [7, 5] })
    const query = render(await issueSearchIdsSql(`login broken is:open`, boards, { backend, limit: 5000 }))
    expect(query.sql).toContain(`websearch_to_tsquery`)
    expect(query.sql).toContain(`union`)
    expect(query.sql).toContain(`unnest(array[`)
    expect(query.params).toEqual(expect.arrayContaining([uuid(7), uuid(5)]))
    const nearest = calls.nearest[0]!
    expect(nearest.k).toBe(ISSUE_SEARCH_SEMANTIC_MAX_HITS)
    expect(nearest.maxDistance).toBe(ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE)
    const filter = render(nearest.filter)
    expect(filter.sql).toContain(`i.id not in (`)
    expect(filter.sql).toContain(`i.status in (`)
  })

  it(`is exactly the lexical subquery when the semantic branch has nothing to add`, async () => {
    const { issueSearchMatchIds } = await import(`@/lib/issue-search-sql`)
    const lexical = render(issueSearchMatchIds(`login broken`, boards, { limit: 5000 }))
    for (const options of [{ available: false }, { nearest: [] }, { embed: async () => null }] satisfies Fake[]) {
      resetIssueSearchVectors()
      const { backend } = fake(options)
      expect(render(await issueSearchIdsSql(`login broken`, boards, { backend, limit: 5000 }))).toEqual(lexical)
    }
    const { backend, calls } = fake({ nearest: [7] })
    expect(render(await issueSearchIdsSql(`EXP-42`, boards, { backend, limit: 5000 }))).toEqual(
      render(issueSearchMatchIds(`EXP-42`, boards, { limit: 5000 }))
    )
    expect(calls.embed).toEqual([])
  })

  it(`similar: is the neighbours alone; none = an empty subquery`, async () => {
    const hit = fake({ source: 12, nearest: [7, 5] })
    const query = render(await issueSearchIdsSql(`similar:NEW-12`, boards, { backend: hit.backend, limit: 5000 }))
    expect(query.sql).not.toContain(`websearch_to_tsquery`)
    expect(query.params).toEqual([uuid(7), uuid(5)])
    expect(hit.calls.nearest[0]!.k).toBe(ISSUE_SEARCH_SIMILAR_MAX_HITS)

    const miss = fake({ source: null })
    expect(render(await issueSearchIdsSql(`similar:NOPE-1 login`, boards, { backend: miss.backend })).sql).toContain(`where false`)
  })
})
