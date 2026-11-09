#!/usr/bin/env bun
// VAPP-289: `bun run --filter @exponential_app/eui-bench counts [--write]`.
// Without flags: checks every derived count budget in budgets.json against
// counts.json + its renderer's offset (what src/counts.test.ts gates).
// `--write`: lets the core recompute counts.json from the committed scenarios
// (`cargo test -p eui-core --test bench_counts` with
// EXPONENTIAL_UI_BENCH_COUNTS=write), then rewrites every derived count
// budget on every machine. Timings, allocations and the counts outside
// `derivedCounts` are left alone (`ratchet --write` owns them).
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { BENCH_ROOT } from "../src/results"
import { applyCounts, describeMismatch, FIX_COMMAND, mismatches, type Counts } from "../src/counts"
import type { Budgets } from "../src/ratchet"

const write = process.argv.includes(`--write`)
const budgetsPath = join(BENCH_ROOT, `budgets.json`)
const countsPath = join(BENCH_ROOT, `counts.json`)

if (write) {
  const cargo = Bun.spawnSync([`cargo`, `test`, `-p`, `eui-core`, `--test`, `bench_counts`], {
    cwd: join(BENCH_ROOT, `../../apps/desktop`),
    env: { ...process.env, EXPONENTIAL_UI_BENCH_COUNTS: `write` },
    stdout: `inherit`,
    stderr: `inherit`,
  })
  if (cargo.exitCode !== 0) process.exit(cargo.exitCode ?? 1)
  console.log(`wrote ${countsPath}`)
}

const budgets = JSON.parse(readFileSync(budgetsPath, `utf8`)) as Budgets
const counts = JSON.parse(readFileSync(countsPath, `utf8`)) as Counts
if (write) {
  writeFileSync(budgetsPath, `${JSON.stringify(applyCounts(budgets, counts), null, 2)}\n`)
  console.log(`wrote ${budgetsPath}`)
}
const wrong = mismatches(write ? (JSON.parse(readFileSync(budgetsPath, `utf8`)) as Budgets) : budgets, counts)
for (const m of wrong) console.error(describeMismatch(m))
if (wrong.length) {
  console.error(write ? `\ncounts: ${wrong.length} budget(s) not derivable (add the renderer's offset to derivedCounts)` : `\ncounts: ${wrong.length} derived count budget(s) stale: run \`${FIX_COMMAND}\``)
  process.exit(1)
}
console.log(`counts: every derived count budget = counts.json + its offset`)
