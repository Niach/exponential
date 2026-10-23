#!/usr/bin/env bun
// VAPP-105: `bun run --filter @exponential-at/ui-bench ratchet <fragment.json | results/<machine>.json>… [--machine <id>] [--write]`
// Checks every fragment against budgets.json (src/ratchet.ts says the rules);
// exit 1 when a metric is worse than its budget or a deterministic one got
// better without its budget. `--write` rewrites budgets.json from the
// fragments (timings × headroom, counts exact), the way the Compose
// conformance budget is rewritten after a fix.
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { BENCH_ROOT, fragmentsOf, readFragment, type Fragment } from "../src/results"
import { budgetsFor, check, describe, failing, type Budgets } from "../src/ratchet"

const args = process.argv.slice(2)
const write = args.includes(`--write`)
const machineAt = args.indexOf(`--machine`)
const machine = machineAt >= 0 ? args[machineAt + 1] : process.env.EXPONENTIAL_UI_BENCH_MACHINE
const files = args.filter((a, i) => !a.startsWith(`--`) && args[i - 1] !== `--machine`)
if (files.length === 0) {
  console.error(`usage: ratchet <fragment.json | results/<machine>.json>… [--machine <id>] [--write]`)
  process.exit(2)
}

const budgetsPath = join(BENCH_ROOT, `budgets.json`)
let budgets = JSON.parse(readFileSync(budgetsPath, `utf8`)) as Budgets

const fragments: Fragment[] = files.flatMap((f) => {
  const parsed = JSON.parse(readFileSync(f, `utf8`))
  return `runs` in parsed ? fragmentsOf(parsed) : [readFragment(f)]
})

let failed = 0
for (const fragment of fragments) {
  const m = machine ?? fragment.machine ?? `unknown`
  const verdicts = check(fragment, budgets, m)
  for (const v of verdicts) console.log(describe(v))
  failed += failing(verdicts).length
  if (write) budgets = budgetsFor(fragment, budgets, m)
}
if (write) {
  writeFileSync(budgetsPath, `${JSON.stringify(budgets, null, 2)}\n`)
  console.log(`wrote ${budgetsPath}`)
  process.exit(0)
}
console.log(failed ? `\nbench: ${failed} metric(s) outside the budget` : `\nbench: every budgeted metric holds`)
process.exit(failed ? 1 : 0)
