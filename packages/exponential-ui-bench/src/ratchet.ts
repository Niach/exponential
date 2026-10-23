// VAPP-105: the RATCHET, in the style of the Compose conformance budgets
// (RealFontConformanceTest): budgets.json holds, per machine × renderer ×
// scenario × metric, the number CI accepts. A metric WORSE than its budget
// fails. A metric BETTER than its budget asks to lower it: a timing (`ms`)
// below `floor` × budget prints a warning (CI runners are noisy, so a better
// timing alone never fails), a deterministic count (`count`, `bytes`) below
// its budget FAILS, as it must land with its new budget in the same change.
// A metric with no budget (a new scenario, a new machine) is reported, never
// gated, so a first run on a new machine passes and writes the numbers.
import manifest from "../scenarios/manifest.json"
import type { Fragment } from "./results"

export type Budgets = {
  $comment?: string
  /** A timing this far below its budget asks to lower it (0.5 = half). */
  floor: number
  /** How far over a measured timing `--write` puts the new budget (1.5 = 50 % headroom). */
  headroom: number
  /** A machine's own `headroom` (a shared hosted VM drifts far more than a dedicated box). */
  machineHeadroom?: Record<string, number>
  machines: Record<string, Record<string, Record<string, Record<string, number>>>>
}

export interface Verdict {
  machine: string
  renderer: string
  scenario: string
  metric: string
  value: number
  budget?: number
  kind: `ms` | `count` | `bytes` | `mb`
  status: `ok` | `worse` | `better` | `unbudgeted`
}

const KINDS: Record<string, Record<string, Verdict[`kind`]>> = Object.fromEntries(manifest.scenarios.map((s) => [s.id, Object.fromEntries(s.metrics.map((m) => [m.id, m.kind as Verdict[`kind`]]))]))

/** Compare one fragment with the budgets of its machine. */
export function check(fragment: Fragment, budgets: Budgets, machine = fragment.machine ?? `unknown`): Verdict[] {
  const out: Verdict[] = []
  const mine = budgets.machines[machine]?.[fragment.renderer] ?? {}
  for (const [scenario, metrics] of Object.entries(fragment.scenarios)) {
    for (const [metric, value] of Object.entries(metrics)) {
      const kind = KINDS[scenario]?.[metric] ?? `ms`
      const budget = mine[scenario]?.[metric]
      let status: Verdict[`status`] = `ok`
      if (budget === undefined) status = `unbudgeted`
      else if (value > budget) status = `worse`
      else if (kind === `ms` || kind === `mb` ? value < budget * budgets.floor : value < budget) status = `better`
      out.push({ machine, renderer: fragment.renderer, scenario, metric, value, budget, kind, status })
    }
  }
  return out
}

/** The verdicts that fail CI: worse, or a deterministic metric that got better. */
export const failing = (verdicts: Verdict[]) => verdicts.filter((v) => v.status === `worse` || (v.status === `better` && v.kind !== `ms` && v.kind !== `mb`))

const timing = (kind: Verdict[`kind`]) => kind === `ms` || kind === `mb`

/** The headroom `--write` gives [machine]'s timings: its own, else the file's. */
export const headroomOf = (budgets: Budgets, machine: string) => budgets.machineHeadroom?.[machine] ?? budgets.headroom

/** Several runs of one renderer on one machine as one fragment: every timing
 *  at its WORST (highest), every count from the last run given. */
export function worstOf(runs: Fragment[]): Fragment {
  const last = runs[runs.length - 1]
  const scenarios: Fragment[`scenarios`] = {}
  for (const [scenario, metrics] of Object.entries(last.scenarios)) {
    scenarios[scenario] = Object.fromEntries(
      Object.entries(metrics).map(([metric, value]) => {
        if (!timing(KINDS[scenario]?.[metric] ?? `ms`)) return [metric, value]
        return [metric, Math.max(value, ...runs.map((r) => r.scenarios[scenario]?.[metric] ?? -Infinity))]
      })
    )
  }
  return { ...last, scenarios }
}

/** `--write`: the budgets a fragment's numbers imply (timings × the machine's headroom, counts exact). */
export function budgetsFor(fragment: Fragment, budgets: Budgets, machine = fragment.machine ?? `unknown`): Budgets {
  const headroom = headroomOf(budgets, machine)
  const next: Budgets = { ...budgets, machines: { ...budgets.machines } }
  const mine = { ...(next.machines[machine] ?? {}) }
  const byScenario: Record<string, Record<string, number>> = {}
  for (const [scenario, metrics] of Object.entries(fragment.scenarios)) {
    byScenario[scenario] = Object.fromEntries(
      Object.entries(metrics).map(([metric, value]) => {
        const kind = KINDS[scenario]?.[metric] ?? `ms`
        return [metric, timing(kind) ? Math.ceil(value * headroom * 10) / 10 : value]
      })
    )
  }
  mine[fragment.renderer] = byScenario
  next.machines[machine] = Object.fromEntries(Object.entries(mine).sort(([a], [b]) => a.localeCompare(b)))
  next.machines = Object.fromEntries(Object.entries(next.machines).sort(([a], [b]) => a.localeCompare(b)))
  return next
}

const fmt = (v: number, kind: Verdict[`kind`]) => (kind === `ms` ? `${v.toFixed(v < 10 ? 2 : 1)} ms` : kind === `mb` ? `${v.toFixed(1)} MB` : kind === `bytes` ? `${Math.round(v).toLocaleString(`en-US`)} B` : String(Math.round(v)))

/** One line per verdict, GitHub Actions annotations for the ones that matter. */
export function describe(v: Verdict): string {
  const where = `${v.renderer} ${v.scenario}.${v.metric}`
  const value = fmt(v.value, v.kind)
  switch (v.status) {
    case `worse`:
      return `::error title=bench worse::${where} = ${value} > budget ${fmt(v.budget!, v.kind)} (${v.machine})`
    case `better`:
      return v.kind === `ms` || v.kind === `mb`
        ? `::warning title=bench better::${where} = ${value}, far under budget ${fmt(v.budget!, v.kind)}: lower the budget in budgets.json (${v.machine})`
        : `::error title=bench better::${where} = ${value} < budget ${fmt(v.budget!, v.kind)}: lower the budget in budgets.json in this change (${v.machine})`
    case `unbudgeted`:
      return `::notice title=bench unbudgeted::${where} = ${value}: no budget for ${v.machine} yet (\`ratchet --write\` records one)`
    default:
      return `ok    ${where} = ${value} (budget ${fmt(v.budget!, v.kind)})`
  }
}
