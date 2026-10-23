#!/usr/bin/env bun
// VAPP-105: the Compose painter on an attached Android device or a running
// emulator: `BenchSuiteInstrumentedTest` through `connectedDebugAndroidTest`
// (the facade's .so for the device's ABI must be in the AAR:
// apps/desktop/crates/exponential-ui-ffi/build-android.sh), the fragment
// read back from logcat (`ExponentialUIBench` lines). No device = a note
// and exit 0, so `bench` runs through on a machine without one.
//   bun run packages/exponential-ui-bench/runners/compose-android.ts
import { join } from "node:path"
import { spawnSync } from "node:child_process"
import { REPO_ROOT, gitCommit, machineInfo, writeFragment, type Fragment } from "../src/results"
import { androidSdk, hostFacade } from "./compose-shared"

const compose = join(REPO_ROOT, `packages/exponential-ui-compose`)
const sdk = androidSdk(compose)
const adb = join(sdk, `platform-tools`, `adb`)
const devices = spawnSync(adb, [`devices`], { encoding: `utf8` })
if (devices.status !== 0) {
  console.log(`compose-android: no adb at ${adb}; skipped`)
  process.exit(0)
}
const attached = devices.stdout.split(`\n`).slice(1).filter((l) => /\tdevice$/.test(l.trim()))
if (attached.length === 0) {
  console.log(`compose-android: no device or emulator attached; skipped`)
  process.exit(0)
}
spawnSync(adb, [`logcat`, `-c`], { stdio: `inherit` })
const r = spawnSync(`./gradlew`, [`:ui-compose:connectedDebugAndroidTest`, `-Pandroid.testInstrumentationRunnerArguments.class=at.exponential.ui.BenchSuiteInstrumentedTest`, `-Pandroid.testInstrumentationRunnerArguments.bench=1`, `--no-daemon`], {
  cwd: compose,
  stdio: `inherit`,
  env: { ...process.env, CARGO_TARGET_DIR: hostFacade(false) },
})
if (r.status !== 0) process.exit(r.status ?? 1)
const log = spawnSync(adb, [`logcat`, `-d`, `-s`, `ExponentialUIBench:I`], { encoding: `utf8` }).stdout
const parts = new Map<number, string>()
let total = 0
for (const m of log.matchAll(/EXPONENTIAL_UI_BENCH_FRAGMENT (\d+)\/(\d+) (.*)$/gm)) {
  parts.set(Number(m[1]), m[3])
  total = Number(m[2])
}
if (total === 0 || parts.size !== total) {
  console.error(`compose-android: no complete fragment in logcat (${parts.size}/${total} chunks)`)
  process.exit(1)
}
const json = Array.from({ length: total }, (_, i) => parts.get(i + 1)!).join(``)
const fragment = JSON.parse(json) as Fragment
fragment.machine = machineInfo().id
fragment.commit = gitCommit()
const path = writeFragment(fragment)
console.log(`compose-android (${fragment.device}) → ${path}`)
console.log(JSON.stringify(fragment.scenarios, null, 1))
