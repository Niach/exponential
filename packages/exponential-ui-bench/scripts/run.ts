#!/usr/bin/env bun
// VAPP-105: `bun run --filter @exponential_app/ui-bench bench [--renderers all|react,solid,element,core,gpui,swift,swift-ios,compose-jvm,compose-android] [--scenario <id>] [--repeat <n>] [--machine <id>] [--no-merge]`
// Runs every runner this machine can (default: React, Solid, the element + the core everywhere,
// gpui on Linux, SwiftUI + the iPhone Simulator on macOS, Compose on the
// JVM when an Android SDK is around, Compose on a device when `adb devices`
// lists one; an attached iPhone makes the Swift iOS runner a device run),
// merges their fragments into results/<machine>.json (`scripts/machine.ts`
// prints the id), regenerates results/index.ts for the Numbers page and
// prints the ratchet's verdicts (never failing here: CI gates). `--merge-only`
// skips the runners and merges every fragment already in .bench/ (CI
// artifacts, or runs done one by one).
import { execSync, spawnSync } from "node:child_process"
import { existsSync, readdirSync, statSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"
import { BENCH_ROOT, REPO_ROOT, SCRATCH, gitCommit, machineInfo, mergeResults, readFragment, writeResults, type Fragment } from "../src/results"
import { writeResultsIndex } from "../src/results-index"

const args = process.argv.slice(2)
const flag = (name: string) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : undefined
}
const scenario = flag(`--scenario`)
const repeat = flag(`--repeat`)
const wanted = flag(`--renderers`) ?? `auto`
const merge = !args.includes(`--no-merge`)
const mergeOnly = args.includes(`--merge-only`)
if (flag(`--machine`)) process.env.EXPONENTIAL_UI_BENCH_MACHINE = flag(`--machine`)

const sh = (cmd: string) => {
  try {
    return execSync(cmd, { stdio: [`ignore`, `pipe`, `ignore`] }).toString()
  } catch {
    return ``
  }
}
const androidSdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT || (existsSync(join(homedir(), `Library/Android/sdk`)) ? join(homedir(), `Library/Android/sdk`) : undefined)
const adbDevices = androidSdk ? sh(`${join(androidSdk, `platform-tools`, `adb`)} devices`).split(`\n`).slice(1).filter((l) => /\tdevice$/.test(l)) : []

function auto(): string[] {
  const list = [`react`, `solid`, `element`, `core`]
  if (process.platform === `linux`) list.push(`gpui`)
  if (process.platform === `darwin`) list.push(`swift`, `swift-ios`)
  if (androidSdk) list.push(`compose-jvm`)
  if (adbDevices.length) list.push(`compose-android`)
  return list
}

const runners = mergeOnly ? [] : wanted === `all` ? [`react`, `solid`, `element`, `core`, `gpui`, `swift`, `swift-ios`, `compose-jvm`, `compose-android`] : wanted === `auto` ? auto() : wanted.split(`,`)
const started = Date.now()
const failed: string[] = []
for (const name of runners) {
  const script = join(BENCH_ROOT, `runners`, `${name}.ts`)
  if (!existsSync(script)) {
    console.error(`bench: no runner ${name} (runners/${name}.ts)`)
    failed.push(name)
    continue
  }
  console.log(`\n== ${name}`)
  const extra = [...(scenario ? [`--scenario`, scenario] : []), ...(repeat ? [`--repeat`, repeat] : [])]
  const r = spawnSync(`bun`, [`run`, script, ...extra], { cwd: REPO_ROOT, stdio: `inherit`, env: { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? join(REPO_ROOT, `apps/desktop/target`) } })
  if (r.status !== 0) failed.push(name)
}

const fragments: Fragment[] = existsSync(SCRATCH)
  ? readdirSync(SCRATCH)
      .filter((f) => f.endsWith(`.json`) && (mergeOnly || statSync(join(SCRATCH, f)).mtimeMs >= started - 1000))
      .map((f) => readFragment(join(SCRATCH, f)))
  : []
const machine = machineInfo()
const commit = gitCommit()
for (const f of fragments) {
  f.machine = machine.id
  f.commit ??= commit
}
console.log(`\nbench: ${fragments.length} fragment(s) on ${machine.id} (${machine.cpu}, ${machine.cores} cores, ${machine.os})${failed.length ? `; failed: ${failed.join(`, `)}` : ``}`)
if (merge && fragments.length) {
  const path = writeResults(mergeResults(machine, fragments))
  console.log(`wrote ${path}`)
  writeResultsIndex()
  spawnSync(`bun`, [`run`, join(BENCH_ROOT, `scripts/ratchet.ts`), path], { stdio: `inherit` })
}
process.exit(failed.length ? 1 : 0)
