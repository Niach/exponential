#!/usr/bin/env bun
// VAPP-105: the gpui painter's runner (apps/desktop/crates/exponential-ui-gpui/examples/bench_suite.rs),
// headless on Linux only (gpui's cosmic-text system with the conformance
// fonts): `bun run runners/gpui.ts [--scenario <id>] [--repeat <n>]` writes
// the `gpui` fragment to `fragmentPath("gpui")`. Elsewhere it exits 0 with
// a note and writes nothing.
import { spawnSync } from "node:child_process"
import { join } from "node:path"
import { fragmentPath, readFragment, REPO_ROOT, writeFragment } from "../src/results"

if (process.platform !== `linux`) {
  console.log(`gpui bench: headless gpui runs on Linux only (skipped on ${process.platform})`)
  process.exit(0)
}
const out = fragmentPath(`gpui`)
const r = spawnSync(`cargo`, [`run`, `--release`, `-p`, `exponential-ui-gpui`, `--example`, `bench_suite`, `--`, ...process.argv.slice(2)], {
  cwd: join(REPO_ROOT, `apps`, `desktop`),
  stdio: [`ignore`, `ignore`, `inherit`],
  env: { ...process.env, EXPONENTIAL_UI_BENCH_OUT: out },
})
if (r.status !== 0) {
  console.error(`gpui bench failed (${r.status ?? r.signal})`)
  process.exit(r.status ?? 1)
}
// The Rust example writes no machine: stamp it.
writeFragment(readFragment(out), out)
console.log(`gpui → ${out}`)
