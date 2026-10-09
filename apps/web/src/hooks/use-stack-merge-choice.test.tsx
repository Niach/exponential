import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

// EXP-1145: the plumbing under the merge button's stack read. The decision
// itself is `stackMergeChoice` (fixture-locked in `lib/pr-stack.test.ts`);
// this proves the hook reaches the team's open rows through the issue's
// board, reports "not ready" until they are in, and stays idle when disarmed.

vi.mock(`@/lib/collections`, () => ({
  boardCollection: {},
  issueCollection: {},
}))

// Each query aliases its source (`i` issues, `b` boards); the stub records
// the alias and answers with that table's rows. The issues table is asked
// twice (the issue by id, then the team's open PRs): the by-id query is told
// apart by its `eq(i.id, …)` predicate, which the probe records too.
const liveRows = vi.hoisted(() => ({
  issues: [] as Record<string, unknown>[],
  boards: [] as Record<string, unknown>[],
  calls: 0,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      let byId: string | null = null
      const chain = {
        where: (predicate: (row: Record<string, unknown>) => unknown) => {
          // Run the predicate against a probe row: `eq(i.id, X)` reads `.id`.
          const seen: string[] = []
          const probeRow = new Proxy(
            {},
            { get: (_target, key) => (seen.push(String(key)), `probe:${String(key)}`) }
          )
          const result = predicate({ i: probeRow, b: probeRow })
          if (seen.includes(`id`) && seen.length === 1) {
            byId = JSON.stringify(result)
          }
          return chain
        },
      }
      const probe = {
        from: (source: Record<string, unknown>) => {
          alias = Object.keys(source)[0] ?? ``
          return chain
        },
      }
      const result = build(probe)
      if (result === undefined) return { data: undefined }
      liveRows.calls += 1
      if (alias === `b`) return { data: liveRows.boards }
      if (byId) {
        const wanted = liveRows.issues.find((row) => byId!.includes(String(row.id)))
        return { data: wanted ? [wanted] : [] }
      }
      return { data: liveRows.issues.filter((row) => row.prState === `open`) }
    },
  }
})

import { useStackMergeConfirm } from "@/hooks/use-stack-merge-choice"

const row = (
  id: string,
  identifier: string,
  branch: string,
  base: string,
  prState = `open`
) => ({
  id,
  identifier,
  boardId: `b1`,
  branch,
  prBaseBranch: base,
  prState,
  prUrl: `https://github.com/o/r/pull/${id}`,
})

describe(`useStackMergeConfirm`, () => {
  it(`is idle and query-free while disarmed`, () => {
    liveRows.calls = 0
    liveRows.issues = [row(`b`, `EXP-1`, `exp/EXP-1`, `master`)]
    liveRows.boards = [{ id: `b1`, teamId: `t1` }]
    const { result } = renderHook(() => useStackMergeConfirm(`b`, false))
    expect(result.current).toEqual({ ready: true, confirm: null })
    expect(liveRows.calls).toBe(0)
  })

  it(`reads the stack through the issue's board once armed`, () => {
    liveRows.issues = [
      row(`b`, `EXP-1`, `exp/EXP-1`, `master`),
      row(`m`, `EXP-2`, `exp/EXP-2`, `exp/EXP-1`),
      row(`t`, `EXP-3`, `exp/EXP-3`, `exp/EXP-2`, `merged`),
    ]
    liveRows.boards = [{ id: `b1`, teamId: `t1` }]
    const { result } = renderHook(() => useStackMergeConfirm(`b`, true))
    expect(result.current.ready).toBe(true)
    expect(result.current.confirm).toMatchObject({
      title: `Merge stack`,
      landing: [`EXP-1`, `EXP-2`],
      input: { issueId: `m`, mergeStack: true },
    })
  })

  it(`answers a lone pull request with a plain merge`, () => {
    liveRows.issues = [row(`b`, `EXP-1`, `exp/EXP-1`, `master`)]
    liveRows.boards = [{ id: `b1`, teamId: `t1` }]
    const { result } = renderHook(() => useStackMergeConfirm(`b`, true))
    expect(result.current).toEqual({ ready: true, confirm: null })
  })

  it(`answers Merge through here on the bottom with the bottom alone`, () => {
    liveRows.issues = [
      row(`b`, `EXP-1`, `exp/EXP-1`, `master`),
      row(`m`, `EXP-2`, `exp/EXP-2`, `exp/EXP-1`),
    ]
    liveRows.boards = [{ id: `b1`, teamId: `t1` }]
    const { result } = renderHook(() => useStackMergeConfirm(`b`, true, `through`))
    expect(result.current.confirm).toMatchObject({
      title: `Merge through here`,
      landing: [`EXP-1`],
      staysOpen: [`EXP-2`],
      input: { issueId: `b`, mergeStack: true },
    })
  })

  it(`is not ready while the issue row has not synced`, () => {
    liveRows.issues = []
    liveRows.boards = [{ id: `b1`, teamId: `t1` }]
    const { result } = renderHook(() => useStackMergeConfirm(`ghost`, true))
    expect(result.current).toEqual({ ready: false, confirm: null })
  })
})
