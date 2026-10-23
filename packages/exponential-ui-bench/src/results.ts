// VAPP-105: the result format every runner writes and the ratchet reads.
//
// A RUNNER writes one FRAGMENT: `{ renderer, platform, device?, machine?,
// date, commit?, scenarios: { <id>: { <metric>: number } } }` to
// $EXPONENTIAL_UI_BENCH_OUT (default `.bench/<renderer>.json` at the repo
// root). `scripts/run.ts` merges fragments into `results/<machine>.json`,
// the committed record of one machine; the ratchet compares fragments (or
// a results file) with budgets.json.
import { execSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { cpus, platform, release, totalmem } from "node:os"
import { dirname, join } from "node:path"

export const REPO_ROOT = join(import.meta.dir, `..`, `..`, `..`)
export const BENCH_ROOT = join(import.meta.dir, `..`)
export const SCRATCH = join(REPO_ROOT, `.bench`)

export type Metrics = Record<string, number>

export interface Fragment {
  renderer: string
  /** `web` | `linux` | `macos` | `ios` | `android` */
  platform: string
  /** The device or simulator the natives ran on (`iPhone 17 Pro Simulator`, `Pixel 8`). */
  device?: string
  machine?: string
  date: string
  commit?: string
  scenarios: Record<string, Metrics>
}

export interface MachineInfo {
  id: string
  cpu: string
  cores: number
  memoryGb: number
  os: string
}

export interface ResultsFile {
  $comment: string
  machine: MachineInfo
  runs: Record<string, Omit<Fragment, `renderer` | `machine`>>
}

const slug = (s: string) => s.toLowerCase().replace(/\(r\)|\(tm\)|cpu|processor|@.*$/g, ``).replace(/[^a-z0-9]+/g, `-`).replace(/^-|-$/g, ``)

/** `EXPONENTIAL_UI_BENCH_MACHINE` (CI: `github-ubuntu-latest`), else the CPU model + the OS. */
export function machineInfo(): MachineInfo {
  const cpu = cpus()[0]?.model ?? `unknown`
  const os = `${platform()} ${release()}`
  const id = process.env.EXPONENTIAL_UI_BENCH_MACHINE || `${slug(cpu)}-${platform()}`
  return { id, cpu: cpu.trim(), cores: cpus().length, memoryGb: Math.round(totalmem() / 2 ** 30), os }
}

export function gitCommit(): string | undefined {
  try {
    return execSync(`git rev-parse --short HEAD`, { cwd: REPO_ROOT, stdio: [`ignore`, `pipe`, `ignore`] }).toString().trim()
  } catch {
    return undefined
  }
}

export const round = (v: number, digits = 2) => Math.round(v * 10 ** digits) / 10 ** digits

/** Where a runner writes its fragment (`EXPONENTIAL_UI_BENCH_OUT` overrides). */
export function fragmentPath(renderer: string): string {
  return process.env.EXPONENTIAL_UI_BENCH_OUT || join(SCRATCH, `${renderer}.json`)
}

/** Every fragment names its machine (the ratchet reads budgets by it; a fragment without one is checked against nothing). */
export function writeFragment(fragment: Fragment, path = fragmentPath(fragment.renderer)): string {
  fragment.machine ??= machineInfo().id
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, `${JSON.stringify(fragment, null, 2)}\n`)
  return path
}

export function readFragment(path: string): Fragment {
  const f = JSON.parse(readFileSync(path, `utf8`)) as Fragment
  if (!f.renderer || !f.scenarios) throw new Error(`${path}: not a bench fragment (renderer + scenarios)`)
  return f
}

export const resultsPath = (machine: string) => join(BENCH_ROOT, `results`, `${machine}.json`)

export function readResults(machine: string): ResultsFile | null {
  const path = resultsPath(machine)
  return existsSync(path) ? (JSON.parse(readFileSync(path, `utf8`)) as ResultsFile) : null
}

/** Merge fragments into the machine's results file (one run per renderer, the newest wins). */
export function mergeResults(machine: MachineInfo, fragments: Fragment[]): ResultsFile {
  const existing = readResults(machine.id)
  const runs = { ...(existing?.runs ?? {}) }
  for (const { renderer, machine: _m, ...run } of fragments) runs[renderer] = run
  return {
    $comment: `Exponential UI benchmark results (VAPP-105) on ONE machine, written by \`bun run --filter @exponential_app/ui-bench bench\`: one run per renderer, every scenario's metrics (scenarios/manifest.json says what each is; ms unless named otherwise). Commit a rerun whole, never edit a number.`,
    machine,
    runs: Object.fromEntries(Object.entries(runs).sort(([a], [b]) => a.localeCompare(b))),
  }
}

export function writeResults(file: ResultsFile): string {
  const path = resultsPath(file.machine.id)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, `${JSON.stringify(file, null, 2)}\n`)
  return path
}

/** A results file as fragments (the ratchet takes either). */
export function fragmentsOf(file: ResultsFile): Fragment[] {
  return Object.entries(file.runs).map(([renderer, run]) => ({ renderer, machine: file.machine.id, ...run }))
}
