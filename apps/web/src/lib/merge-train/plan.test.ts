import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/merge-train.json"
import type { TrainVerdict } from "@exp/domain-contract/merge-train"
import { buildUnits, decideRun, queuePositions, subtreeIdle, type PlanRun, type RunDecision } from "@/lib/merge-train/plan"

// EXP-1709: the worker's run sequence replays merge-train.json `bisect`
// (the contract's `trainRun` oracle) case by case.

interface BisectCase {
  name: string
  input: { batch: number[][]; culprits: number[]; flaky?: number }
  expected: { runs: number[][]; landed: number[]; failing: number[] }
}

function replay(input: BisectCase[`input`]) {
  const culprits = new Set(input.culprits)
  let flaky = input.flaky ?? 0
  const runs: number[][] = []
  const landed: number[] = []
  const failing: number[] = []
  let run: PlanRun<number> | null = { units: input.batch, attempt: 0, bisect: false, pending: [] }
  while (run) {
    const prs = run.units.flat()
    runs.push(prs)
    let red = prs.some((n) => culprits.has(n))
    if (flaky > 0) {
      flaky -= 1
      red = true
    }
    const verdict: Pick<TrainVerdict, `state` | `composed` | `conflicts`> = {
      state: red ? `red` : `green`,
      composed: run.units,
      conflicts: [],
    }
    const unitOf = (numbers: number[]) => numbers
    const decision: RunDecision<number> = decideRun(run, verdict, unitOf)!
    landed.push(...decision.land.flat())
    failing.push(...decision.failing.flat())
    run = decision.next
  }
  return { runs, landed, failing }
}

describe(`decideRun replays merge-train.json bisect`, () => {
  for (const c of fixture.bisect as BisectCase[]) {
    it(c.name, () => {
      expect(replay(c.input)).toEqual(c.expected)
    })
  }

  it(`bounces conflicts whatever the state; an error requeues the run and its pending halves`, () => {
    const run: PlanRun<string> = { units: [[`a`], [`b`]], attempt: 0, bisect: false, pending: [[[`c`]]] }
    const unitOf = (numbers: number[]) => (numbers[0] === 1 ? [`a`] : numbers[0] === 2 ? [`b`] : null)
    const green = decideRun(run, { state: `green`, composed: [[1]], conflicts: [{ prs: [2], files: [`x.ts`] }] }, unitOf)!
    expect(green.land).toEqual([[`a`]])
    expect(green.conflicts).toEqual([{ unit: [`b`], files: [`x.ts`] }])
    expect(green.next).toEqual({ units: [[`c`]], attempt: 1, bisect: true, pending: [] })
    const error = decideRun(run, { state: `error`, composed: [], conflicts: [] }, unitOf)!
    expect(error.requeue).toEqual([[`a`], [`b`], [`c`]])
    expect(decideRun(run, { state: `pending`, composed: [], conflicts: [] }, unitOf)).toBeNull()
  })
})

describe(`buildUnits`, () => {
  const at = (n: number) => new Date(Date.UTC(2027, 0, 1, 0, n))
  it(`groups a stack bottom first and keeps lone PRs alone, oldest first`, () => {
    const prs = [
      { id: `top`, number: 12, basePullRequestId: `mid`, createdAt: at(3) },
      { id: `lone`, number: 9, basePullRequestId: null, createdAt: at(2) },
      { id: `mid`, number: 11, basePullRequestId: `bottom`, createdAt: at(2) },
      { id: `bottom`, number: 10, basePullRequestId: null, createdAt: at(1) },
      { id: `orphan`, number: 13, basePullRequestId: `merged-gone`, createdAt: at(4) },
    ]
    expect(buildUnits(prs).map((u) => u.map((p) => p.number))).toEqual([[10, 11, 12], [9], [13]])
  })
})

describe(`subtreeIdle`, () => {
  const s = (id: string, parentId: string | null, status = `idle`, busy = false) => ({ id, parentId, status, busy })
  it(`waits for the opener and its descendants only, never the rest of the thread`, () => {
    const tree = [s(`root`, null, `running`, true), s(`opener`, `root`), s(`kid`, `opener`), s(`sibling`, `root`, `queued`)]
    expect(subtreeIdle(`opener`, tree)).toBe(true)
    expect(subtreeIdle(`opener`, [...tree.slice(0, 2), s(`kid`, `opener`, `running`, true), tree[3]!])).toBe(false)
    expect(subtreeIdle(`opener`, [tree[0]!, s(`opener`, `root`, `queued`)])).toBe(false)
    expect(subtreeIdle(`opener`, [s(`opener`, null, `ended`, true)])).toBe(true)
    expect(subtreeIdle(null, tree)).toBe(true)
    expect(subtreeIdle(`gone`, tree)).toBe(true)
  })
})

describe(`queuePositions`, () => {
  it(`shares one place across a stack`, () => {
    expect([...queuePositions([[`a`, `b`], [`c`]])]).toEqual([
      [`a`, 1],
      [`b`, 1],
      [`c`, 2],
    ])
  })
})
