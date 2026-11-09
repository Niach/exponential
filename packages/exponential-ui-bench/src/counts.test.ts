// VAPP-289: the PR gate for the bench's count budgets. counts.json is the
// core's canonical count per scenario (eui-core tests/bench_counts.rs keeps
// it current on the same PR); every count budget budgets.json derives must
// equal it plus its renderer's offset, on every machine.
import { describe, expect, it } from "bun:test"
import budgetsJson from "../budgets.json"
import countsJson from "../counts.json"
import { applyCounts, describeMismatch, derivedValue, FIX_COMMAND, mismatches, type Counts } from "./counts"
import type { Budgets } from "./ratchet"

const budgets = budgetsJson as unknown as Budgets
const counts = countsJson as unknown as Counts

describe(`derived count budgets (VAPP-289)`, () => {
  it(`every derived count budget = counts.json + its renderer's offset`, () => {
    const wrong = mismatches(budgets, counts).map(describeMismatch)
    expect(wrong, `stale count budgets: run \`${FIX_COMMAND}\` and commit counts.json + budgets.json`).toEqual([])
  })

  it(`every derived metric names a count counts.json holds, and a renderer at most once`, () => {
    for (const [key, byCanonical] of Object.entries(budgets.derivedCounts ?? {})) {
      const [scenario] = key.split(`.`)
      const seen = new Set<string>()
      for (const [canonical, renderers] of Object.entries(byCanonical)) {
        expect(typeof (counts[scenario] as Record<string, number> | undefined)?.[canonical], `${key} ← ${canonical}`).toBe(`number`)
        for (const r of Object.keys(renderers)) {
          expect(seen.has(r), `${key}: ${r} twice`).toBe(false)
          seen.add(r)
        }
      }
    }
  })

  it(`a budget one off fails, and applyCounts rewrites only derived counts`, () => {
    const [machine, renderer] = [`github-ubuntu-latest`, `react-chromium`]
    const bumped: Budgets = structuredClone(budgets)
    const cold = bumped.machines[machine][renderer][`cold-start`]
    cold.nodes += 1
    cold.firstPaintMs = 12345
    expect(mismatches(bumped, counts).map((m) => `${m.machine} ${m.renderer} ${m.scenario}.${m.metric}`)).toEqual([`${machine} ${renderer} cold-start.nodes`])
    const fixed = applyCounts(bumped, counts)
    expect(mismatches(fixed, counts)).toEqual([])
    expect(fixed.machines[machine][renderer][`cold-start`].firstPaintMs).toBe(12345)
    expect(fixed.machines[machine][renderer][`cold-start`].nodes).toBe(derivedValue(budgets, counts, renderer, `cold-start`, `nodes`).want!)
  })
})
