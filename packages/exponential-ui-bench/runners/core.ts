#!/usr/bin/env bun
// VAPP-105: the Rust core's runner (apps/desktop/crates/exponential-ui/examples/bench_suite.rs):
// `bun run runners/core.ts [--scenario <id>] [--repeat <n>]` builds it in release
// and writes the `core` fragment to `fragmentPath("core")`.
import { spawnSync } from "node:child_process"
import { join } from "node:path"
import { fragmentPath, REPO_ROOT } from "../src/results"

const out = fragmentPath(`core`)
const r = spawnSync(`cargo`, [`run`, `--release`, `-p`, `exponential-ui`, `--example`, `bench_suite`, `--`, ...process.argv.slice(2)], {
  cwd: join(REPO_ROOT, `apps`, `desktop`),
  stdio: [`ignore`, `ignore`, `inherit`],
  env: { ...process.env, EXPONENTIAL_UI_BENCH_OUT: out },
})
if (r.status !== 0) {
  console.error(`core bench failed (${r.status ?? r.signal})`)
  process.exit(r.status ?? 1)
}
console.log(`core → ${out}`)
