import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  findRelationCycle,
  relationCycleMessage,
} from "@/lib/relation-cycles"

// EXP-980 — the transitive walker behind relations.create's cycle guard. The
// fake executor answers each frontier query from an in-memory edge list, so
// the walk itself (not a scripted queue) is what is under test.
type Edge = [from: string, to: string]

function executorOver(edges: Edge[]) {
  const queries: string[][] = []
  const executor = {
    select: () => ({
      from: () => ({
        where: async (_condition: unknown) => {
          const frontier = pendingFrontier.shift() ?? []
          queries.push(frontier)
          return edges
            .filter(([from]) => frontier.includes(from))
            .map(([from, to]) => ({ from, to }))
        },
      }),
    }),
  }
  // The condition object is opaque drizzle SQL; the test replays the frontier
  // the walker MUST be on instead: breadth-first levels from the start node.
  const pendingFrontier: string[][] = []
  return { executor, queries, pendingFrontier }
}

/** Breadth-first levels from `start`, the frontiers a correct walk asks for. */
function levels(edges: Edge[], start: string, stopAt: string): string[][] {
  const out: string[][] = []
  const seen = new Set([start])
  let frontier = [start]
  while (frontier.length > 0) {
    out.push(frontier)
    const next: string[] = []
    let hit = false
    for (const [from, to] of [...edges].sort((a, b) => (a[1] < b[1] ? -1 : 1))) {
      if (!frontier.includes(from) || seen.has(to)) continue
      seen.add(to)
      if (to === stopAt) hit = true
      next.push(to)
    }
    if (hit) break
    frontier = next
  }
  return out
}

async function walk(edges: Edge[], from: string, to: string) {
  const h = executorOver(edges)
  h.pendingFrontier.push(...levels(edges, to, from))
  const cycle = await findRelationCycle(h.executor as never, {
    issueId: from,
    relatedIssueId: to,
    type: `blocks`,
  })
  return { cycle, queries: h.queries }
}

describe(`findRelationCycle`, () => {
  it(`accepts an edge into an unrelated graph`, async () => {
    const { cycle } = await walk([[`b`, `c`]], `a`, `b`)
    expect(cycle).toBeNull()
  })

  it(`names the direct reverse`, async () => {
    const { cycle } = await walk([[`b`, `a`]], `a`, `b`)
    expect(cycle).toEqual([`a`, `b`, `a`])
  })

  it(`names a transitive cycle in walking order`, async () => {
    const { cycle } = await walk(
      [
        [`b`, `c`],
        [`c`, `d`],
        [`d`, `a`],
      ],
      `a`,
      `b`
    )
    expect(cycle).toEqual([`a`, `b`, `c`, `d`, `a`])
  })

  it(`names a SHORTEST cycle through a diamond`, async () => {
    const { cycle } = await walk(
      [
        [`b`, `c`],
        [`b`, `d`],
        [`c`, `e`],
        [`e`, `a`],
        [`d`, `a`],
      ],
      `a`,
      `b`
    )
    expect(cycle).toEqual([`a`, `b`, `d`, `a`])
  })

  it(`terminates on a graph that already holds a cycle elsewhere`, async () => {
    const { cycle } = await walk(
      [
        [`b`, `c`],
        [`c`, `b`],
      ],
      `a`,
      `b`
    )
    expect(cycle).toBeNull()
  })

  it(`treats a self edge as a cycle without a query`, async () => {
    const { cycle, queries } = await walk([], `a`, `a`)
    expect(cycle).toEqual([`a`, `a`])
    expect(queries).toEqual([])
  })
})

describe(`relationCycleMessage`, () => {
  it(`keeps the old sentence for the direct reverse`, () => {
    expect(relationCycleMessage(`blocks`, [`EXP-1`, `EXP-2`, `EXP-1`])).toBe(
      `The opposite relation already exists (EXP-1 → EXP-2 → EXP-1)`
    )
  })

  it(`spells a longer cycle out per type`, () => {
    expect(
      relationCycleMessage(`parent`, [`EXP-1`, `EXP-2`, `EXP-3`, `EXP-1`])
    ).toBe(
      `This would create a cycle: EXP-1 → EXP-2 → EXP-3 → EXP-1 (each is the parent of the next)`
    )
  })
})
