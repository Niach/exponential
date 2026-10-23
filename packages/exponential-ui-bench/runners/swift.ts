#!/usr/bin/env bun
// VAPP-105: the SwiftUI painter's bench on macOS: `swift test -c release
// --filter BenchSuiteTests` (Tests/ExponentialUITests/BenchSuiteTests.swift)
// under EXPONENTIAL_UI_BENCH=1, the fragment read from the test's
// `EXPONENTIAL_UI_BENCH_FRAGMENT <json>` line (or the file it wrote) and
// filed as `swift-macos`. Needs the xcframework
// (`bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh`).
//   bun run runners/swift.ts [--repeat <n>]
import { spawnSync } from "node:child_process"
import { existsSync, readFileSync, rmSync } from "node:fs"
import { join } from "node:path"
import { REPO_ROOT, fragmentPath, gitCommit, machineInfo, round, writeFragment, type Fragment } from "../src/results"

export const SWIFT_PKG = join(REPO_ROOT, `packages`, `exponential-ui-swift`)
export const FRAGMENT_LINE = /^EXPONENTIAL_UI_BENCH_FRAGMENT (\{.*\})\s*$/m

export function repeatArg(args: string[], fallback = `3`): string {
  const at = args.indexOf(`--repeat`)
  return at >= 0 ? args[at + 1] : fallback
}

/** The fragment the Swift test produced: the file it wrote when it could, else the stdout line. */
export function fragmentFrom(output: string, path: string): Fragment {
  if (existsSync(path)) return JSON.parse(readFileSync(path, `utf8`)) as Fragment
  const line = FRAGMENT_LINE.exec(output)
  if (!line) throw new Error(`no EXPONENTIAL_UI_BENCH_FRAGMENT line in the test output`)
  return JSON.parse(line[1]) as Fragment
}

/** Fill the machine and the commit and write the fragment where the orchestrator expects it. */
export function file(fragment: Fragment): string {
  fragment.machine = machineInfo().id
  fragment.commit = gitCommit()
  // JSONSerialization prints doubles with their binary noise (175.86199999999999).
  for (const metrics of Object.values(fragment.scenarios)) for (const k of Object.keys(metrics)) metrics[k] = round(metrics[k], 3)
  return writeFragment(fragment, fragmentPath(fragment.renderer))
}

if (import.meta.main) {
  if (!existsSync(join(SWIFT_PKG, `Binaries`, `ExponentialUIFFI.xcframework`))) {
    console.error(`swift bench: no xcframework; run bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh first`)
    process.exit(2)
  }
  const out = fragmentPath(`swift-macos`)
  rmSync(out, { force: true })
  const env = { ...process.env, EXPONENTIAL_UI_BENCH: `1`, EXPONENTIAL_UI_BENCH_REPEAT: repeatArg(process.argv.slice(2)), EXPONENTIAL_UI_BENCH_OUT: out }
  const r = spawnSync(`swift`, [`test`, `-c`, `release`, `-Xswiftc`, `-enable-testing`, `--filter`, `BenchSuiteTests`], { cwd: SWIFT_PKG, env, encoding: `utf8`, maxBuffer: 1 << 28 })
  const output = `${r.stdout ?? ``}\n${r.stderr ?? ``}`
  process.stdout.write(output.split(`\n`).filter((l) => l.startsWith(`bench `) || /error|failed|Executed/.test(l)).join(`\n`) + `\n`)
  if (r.status !== 0) {
    console.error(output.slice(-4000))
    process.exit(r.status ?? 1)
  }
  const fragment = fragmentFrom(output, out)
  console.log(`wrote ${file(fragment)}`)
  console.log(JSON.stringify(fragment))
}
