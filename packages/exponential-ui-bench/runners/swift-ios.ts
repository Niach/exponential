#!/usr/bin/env bun
// VAPP-105: the SwiftUI painter's bench on iOS: `xcodebuild test` of
// BenchSuiteTests on an iPhone Simulator (default: the booted one, else
// the newest available iPhone) or an attached device (`--device <udid>`,
// or `--device` alone = the first device `xcrun xctrace list devices`
// shows; the renderer is then `swift-ios-device`). The env reaches the
// test process with xcodebuild's `TEST_RUNNER_` prefix. A DEVICE cannot
// read the repo, so the scenario files are copied into the test target's
// Resources/bench/ for the run and removed after (gitignored). Device runs
// need a signing team: `EXPONENTIAL_UI_TEAM=<id>` (`-allowProvisioningUpdates`).
//   bun run runners/swift-ios.ts [--repeat <n>] [--device [<udid>]] [--simulator <udid>]
import { spawnSync } from "node:child_process"
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs"
import { join } from "node:path"
import { BENCH_ROOT, fragmentPath } from "../src/results"
import { SWIFT_PKG, file, fragmentFrom, repeatArg } from "./swift"

type Sim = { udid: string; name: string; state: string; runtime: string }

function simulators(): Sim[] {
  const r = spawnSync(`xcrun`, [`simctl`, `list`, `devices`, `available`, `-j`], { encoding: `utf8` })
  const devices = (JSON.parse(r.stdout) as { devices: Record<string, { udid: string; name: string; state: string }[]> }).devices
  return Object.entries(devices)
    .filter(([runtime]) => runtime.includes(`iOS`))
    .flatMap(([runtime, list]) => list.filter((d) => d.name.startsWith(`iPhone`)).map((d) => ({ ...d, runtime })))
}

/** The booted iPhone, else the newest runtime's first iPhone. */
function pickSimulator(): Sim {
  const sims = simulators()
  if (sims.length === 0) throw new Error(`no iPhone Simulator available (xcrun simctl list devices available)`)
  const booted = sims.find((s) => s.state === `Booted`)
  if (booted) return booted
  const version = (s: Sim) => (s.runtime.match(/iOS-(\d+)-(\d+)/) ?? [`0`, `0`, `0`]).slice(1).map(Number)
  return sims.sort((a, b) => version(b)[0] - version(a)[0] || version(b)[1] - version(a)[1])[0]
}

/** The first attached (not offline) device of `xcrun xctrace list devices`. */
function pickDevice(): string {
  const r = spawnSync(`xcrun`, [`xctrace`, `list`, `devices`], { encoding: `utf8` })
  const section = r.stdout.split(`== Devices Offline ==`)[0].split(`== Devices ==`)[1] ?? ``
  const line = section.split(`\n`).map((l) => l.trim()).find((l) => /\(\d+\.\d+(\.\d+)?\) \([0-9A-F-]+\)$/.test(l) && !/^Mac/i.test(l) && !/MacBook|iMac|Mac mini|Mac Studio|Mac Pro/.test(l))
  if (!line) throw new Error(`no attached iOS device in xcrun xctrace list devices`)
  return line.match(/\(([0-9A-F-]+)\)$/)![1]
}

const RESOURCES = join(SWIFT_PKG, `Tests`, `ExponentialUITests`, `Resources`, `bench`)

function copyScenarios(): string[] {
  const src = join(BENCH_ROOT, `scenarios`)
  mkdirSync(RESOURCES, { recursive: true })
  const copied: string[] = []
  for (const f of readdirSync(src).filter((f) => f.endsWith(`.json`))) {
    copyFileSync(join(src, f), join(RESOURCES, f))
    copied.push(join(RESOURCES, f))
  }
  return copied
}

if (import.meta.main) {
  const args = process.argv.slice(2)
  if (!existsSync(join(SWIFT_PKG, `Binaries`, `ExponentialUIFFI.xcframework`))) {
    console.error(`swift-ios bench: no xcframework; run bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh first`)
    process.exit(2)
  }
  const deviceAt = args.indexOf(`--device`)
  const onDevice = deviceAt >= 0
  const udid = onDevice ? (args[deviceAt + 1] && !args[deviceAt + 1].startsWith(`--`) ? args[deviceAt + 1] : pickDevice()) : (args.includes(`--simulator`) ? args[args.indexOf(`--simulator`) + 1] : pickSimulator().udid)
  const renderer = onDevice ? `swift-ios-device` : `swift-ios-simulator`
  const destination = onDevice ? `platform=iOS,id=${udid}` : `platform=iOS Simulator,id=${udid}`
  const out = fragmentPath(renderer)
  rmSync(out, { force: true })
  const derived = join(process.env.TMPDIR ?? `/tmp`, `exponential-ui-bench-dd`)
  const copied = onDevice ? copyScenarios() : []
  const env = { ...process.env, TEST_RUNNER_EXPONENTIAL_UI_BENCH: `1`, TEST_RUNNER_EXPONENTIAL_UI_BENCH_REPEAT: repeatArg(args), TEST_RUNNER_EXPONENTIAL_UI_BENCH_OUT: out }
  const xcargs = [`test`, `-scheme`, `ExponentialUI-Package`, `-destination`, destination, `-only-testing:ExponentialUITests/BenchSuiteTests`, `-configuration`, `Release`, `ENABLE_TESTABILITY=YES`, `-derivedDataPath`, derived]
  if (onDevice) {
    xcargs.push(`-allowProvisioningUpdates`)
    if (process.env.EXPONENTIAL_UI_TEAM) xcargs.push(`DEVELOPMENT_TEAM=${process.env.EXPONENTIAL_UI_TEAM}`)
  } else xcargs.push(`CODE_SIGNING_ALLOWED=NO`)
  console.log(`xcodebuild ${xcargs.join(` `)}`)
  let r
  try {
    r = spawnSync(`xcodebuild`, xcargs, { cwd: SWIFT_PKG, env, encoding: `utf8`, maxBuffer: 1 << 28 })
  } finally {
    for (const f of copied) rmSync(f, { force: true })
  }
  const output = `${r.stdout ?? ``}\n${r.stderr ?? ``}`
  process.stdout.write(output.split(`\n`).filter((l) => /^bench |error:|\*\* TEST|Executed/.test(l)).join(`\n`) + `\n`)
  if (r.status !== 0) {
    console.error(output.slice(-6000))
    process.exit(r.status ?? 1)
  }
  const fragment = fragmentFrom(output, out)
  fragment.renderer = renderer
  console.log(`wrote ${file(fragment)}`)
  console.log(JSON.stringify(fragment))
}
