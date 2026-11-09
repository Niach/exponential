// VAPP-289: the count budgets DERIVED from the core. The scenarios paint the
// kitchen sink (every catalog component once), so a catalog change moves the
// deterministic counts (cold-start nodes, theme-switch restyled, …), and
// until now only the release suite measured them. `counts.json` holds the
// canonical counts the core computes from the committed scenarios
// (apps/desktop/crates/eui-core/tests/bench_counts.rs, on the PR's
// `cargo test -p eui-core`); budgets.json's `derivedCounts` says, per
// `<scenario>.<metric>`, which canonical count each renderer reports and its
// fixed offset from it. Every machine's budget for that renderer must equal
// canonical + offset (src/counts.test.ts, on every PR). Counts outside
// `derivedCounts` (layout- or renderer-specific: upcalls, measure and FFI
// calls, windowed rows, allocations) stay with `ratchet --write`.
import type { Budgets } from "./ratchet"

/** counts.json: scenario id → canonical count id → value. */
export type Counts = Record<string, Record<string, number> | string>

/** `<scenario>.<metric>` → canonical count id (in the same scenario) → renderer → offset. */
export type DerivedCounts = Record<string, Record<string, Record<string, number>>>

export const FIX_COMMAND = `bun run --filter @exponential_app/eui-bench counts --write`

export interface CountMismatch {
  machine: string
  renderer: string
  scenario: string
  metric: string
  budget: number | undefined
  want: number | undefined
  why: string
}

/** The renderer → [canonical, offset] map of one derived metric. */
function sources(derived: DerivedCounts, key: string): Map<string, [string, number]> {
  const out = new Map<string, [string, number]>()
  for (const [canonical, renderers] of Object.entries(derived[key] ?? {})) {
    if (canonical.startsWith(`$`)) continue
    for (const [renderer, offset] of Object.entries(renderers)) out.set(renderer, [canonical, offset])
  }
  return out
}

/** The value budgets.json must hold for [renderer]'s `<scenario>.<metric>`, or why it cannot be derived. */
export function derivedValue(budgets: Budgets, counts: Counts, renderer: string, scenario: string, metric: string): { want?: number; why?: string; derived: boolean } {
  const key = `${scenario}.${metric}`
  if (!budgets.derivedCounts?.[key]) return { derived: false }
  const source = sources(budgets.derivedCounts, key).get(renderer)
  if (!source) return { derived: true, why: `${renderer} has no offset in derivedCounts["${key}"]` }
  const [canonical, offset] = source
  const value = (counts[scenario] as Record<string, number> | undefined)?.[canonical]
  if (typeof value !== `number`) return { derived: true, why: `counts.json has no ${scenario}.${canonical}` }
  return { derived: true, want: value + offset }
}

/** Every derived count budget that differs from canonical + offset. */
export function mismatches(budgets: Budgets, counts: Counts): CountMismatch[] {
  const out: CountMismatch[] = []
  for (const [machine, renderers] of Object.entries(budgets.machines)) {
    for (const [renderer, scenarios] of Object.entries(renderers)) {
      for (const [scenario, metrics] of Object.entries(scenarios)) {
        for (const [metric, budget] of Object.entries(metrics)) {
          const d = derivedValue(budgets, counts, renderer, scenario, metric)
          if (!d.derived) continue
          if (d.why) out.push({ machine, renderer, scenario, metric, budget, want: undefined, why: d.why })
          else if (d.want !== budget) out.push({ machine, renderer, scenario, metric, budget, want: d.want, why: `budget ${budget} ≠ canonical + offset = ${d.want}` })
        }
      }
    }
  }
  return out
}

/** budgets.json with every derived count budget rewritten from counts.json (timings and other counts untouched). */
export function applyCounts(budgets: Budgets, counts: Counts): Budgets {
  const machines: Budgets[`machines`] = {}
  for (const [machine, renderers] of Object.entries(budgets.machines)) {
    machines[machine] = {}
    for (const [renderer, scenarios] of Object.entries(renderers)) {
      machines[machine][renderer] = {}
      for (const [scenario, metrics] of Object.entries(scenarios)) {
        machines[machine][renderer][scenario] = Object.fromEntries(
          Object.entries(metrics).map(([metric, budget]) => {
            const d = derivedValue(budgets, counts, renderer, scenario, metric)
            return [metric, d.want ?? budget]
          })
        )
      }
    }
  }
  return { ...budgets, machines }
}

export const describeMismatch = (m: CountMismatch) => `${m.machine} ${m.renderer} ${m.scenario}.${m.metric}: ${m.why}`
