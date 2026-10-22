#!/usr/bin/env bun
// VAPP-105: the Compose painter on the JVM (Robolectric, the host build of
// the facade through JNA): `BenchSuiteTest` under EXPONENTIAL_UI_BENCH=1,
// then its `build/bench-suite.json` as the `compose-jvm` fragment.
//   bun run packages/exponential-ui-bench/runners/compose-jvm.ts [--repeat 3]
import { existsSync } from "node:fs"
import { join } from "node:path"
import { spawnSync } from "node:child_process"
import { REPO_ROOT, gitCommit, machineInfo, readFragment, writeFragment } from "../src/results"
import { androidSdk, hostFacade, repeatArg } from "./compose-shared"

const compose = join(REPO_ROOT, `packages/exponential-ui-compose`)
const target = hostFacade()
androidSdk(compose)
const r = spawnSync(`./gradlew`, [`:ui-compose:testDebugUnitTest`, `--tests`, `*BenchSuiteTest`, `--rerun`, `--no-daemon`], {
  cwd: compose,
  stdio: `inherit`,
  env: { ...process.env, EXPONENTIAL_UI_BENCH: `1`, EXPONENTIAL_UI_BENCH_REPEAT: String(repeatArg()), CARGO_TARGET_DIR: target },
})
if (r.status !== 0) process.exit(r.status ?? 1)
const out = join(compose, `ui-compose/build/bench-suite.json`)
if (!existsSync(out)) {
  console.error(`compose-jvm: ${out} missing (the test skipped?)`)
  process.exit(1)
}
const fragment = readFragment(out)
fragment.machine = machineInfo().id
fragment.commit = gitCommit()
const path = writeFragment(fragment)
console.log(`compose-jvm → ${path}`)
console.log(JSON.stringify(fragment.scenarios, null, 1))
