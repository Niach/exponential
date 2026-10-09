/**
 * The lane scheduler behind `bun run shots` (EXP-1267).
 *
 * The capture lanes used to run one after another: web, desktop, iOS, android.
 * Most of that wall clock is other people's programs waiting on their own
 * device (a headless Chromium, a gpui window, a simulator, an emulator), so
 * they now run CONCURRENTLY, and only the pairs that genuinely share something
 * are kept apart.
 *
 * "Share something" is declared, never inferred: every lane names the
 * RESOURCES it holds, each `exclusive` (nobody else may hold it) or `shared`
 * (any number of shared holders, no exclusive one). That is a readers-writer
 * lock per key, and the reasons are documented where the keys are
 * ([`RESOURCE_REASONS`]), so the plan can print WHY two lanes do not overlap.
 *
 * Scheduling is FIFO by declaration order: a lane that is blocked still CLAIMS
 * its resources, so a later lane can never jump ahead of an earlier one on the
 * same key. That keeps chains ordered (the iOS package app before the fastlane
 * lanes on the same simulator) and makes `--serial` reproduce a fixed order.
 *
 * A lane may also wait on a GATE (the shared seed + relay + demo ids): it does
 * not start before the promise resolves, and if it rejects the lane fails with
 * that reason without running. The backend-free `package` lanes carry no gate,
 * so they build and capture while the seed is still running.
 *
 * One lane's failure never stops another: `run` errors are caught per lane and
 * reported, and every lane is awaited before the scheduler returns.
 */
import { AsyncLocalStorage } from "node:async_hooks"
import { format } from "node:util"

/** Why a key exists: what two lanes would break by holding it at once. */
export const RESOURCE_REASONS = {
  /**
   * The desktop lane photographs a REGION of the real screen
   * (`screencapture -R`), so it alone may own the foreground. Every other lane
   * is headless (Chromium, `simctl`, fastlane snapshot) or already running in
   * its own window (the emulator), so nothing else takes this key.
   */
  screen: `the desktop lane photographs a region of the foreground screen`,
  /**
   * The demo user's `devices` rows. The desktop lane REGISTERS the machine it
   * runs on (and the onboarding identities' scratch machines) for its whole
   * run and prunes them after; a fleet view photographed meanwhile on another
   * lane (Devices, Add server, the composer's device picker, …) shows an
   * extra "Mac mini" the store never holds. Desktop = exclusive, every lane
   * with a fleet view in scope = shared.
   */
  fleet: `the desktop lane registers its own machine in the demo user's device fleet while it runs`,
  /**
   * `capture:views` writes ONE results file (`.shots-raw/capture-views.json`)
   * and truncates it on entry, so two browser passes must not overlap.
   */
  "capture-views": `capture:views owns one results file, .shots-raw/capture-views.json`,
  /**
   * The iOS lanes share ONE simulator (the package app and both Snapfiles
   * name `iPhone 17 Pro Max`, and snapshot ERASES it), plus `tuist generate`
   * and the default DerivedData of apps/ios. Split this key per lane once each
   * fastlane lane has its own simulator and derived data.
   */
  "ios-simulator": `the iOS lanes share one simulator (erased by snapshot), tuist generate and DerivedData`,
  /**
   * Preflight insists on exactly ONE attached adb device, and the package app,
   * screengrab, demo mode and the autofill toggle all drive it.
   */
  "adb-device": `the android lanes share the one attached emulator`,
} as const

export type ResourceKey = keyof typeof RESOURCE_REASONS

export interface ResourceClaim {
  key: ResourceKey
  mode: `exclusive` | `shared`
}

export interface LaneSpec {
  /** Unique, short: `web`, `ios:store`, … — also the log prefix. */
  id: string
  /** One line for the plan: what the lane photographs. */
  summary: string
  resources: ResourceClaim[]
  /** Started only once this resolves; a rejection fails the lane unrun. */
  gate?: Promise<unknown>
  /** A human name for the gate, for the plan (`seed`). */
  gateLabel?: string
  run: () => Promise<void>
}

export interface LaneRun {
  id: string
  ok: boolean
  /** Set when `run` threw, or the gate rejected. */
  error?: string
  /** False when the gate failed and `run` was never called. */
  started: boolean
  /** Wall clock of the lane itself (0 if it never started). */
  ms: number
}

export const UNLIMITED = Number.POSITIVE_INFINITY

function conflicts(a: ResourceClaim[`mode`], b: ResourceClaim[`mode`]): boolean {
  return a === `exclusive` || b === `exclusive`
}

/** Would `claims` collide with anything in `held`? Returns the first key that does. */
function collision(
  claims: ResourceClaim[],
  held: Map<ResourceKey, ResourceClaim[`mode`][]>
): ResourceKey | undefined {
  for (const claim of claims) {
    const holders = held.get(claim.key) ?? []
    if (holders.some((mode) => conflicts(mode, claim.mode))) return claim.key
  }
  return undefined
}

function hold(held: Map<ResourceKey, ResourceClaim[`mode`][]>, claims: ResourceClaim[]): void {
  for (const claim of claims) held.set(claim.key, [...(held.get(claim.key) ?? []), claim.mode])
}

function release(held: Map<ResourceKey, ResourceClaim[`mode`][]>, claims: ResourceClaim[]): void {
  for (const claim of claims) {
    const holders = [...(held.get(claim.key) ?? [])]
    const index = holders.indexOf(claim.mode)
    if (index !== -1) holders.splice(index, 1)
    if (holders.length === 0) held.delete(claim.key)
    else held.set(claim.key, holders)
  }
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Run every lane, at most `jobs` at once, honouring resources, gates and
 * declaration order. Resolves once EVERY lane has finished (or failed); never
 * rejects. Results come back in declaration order.
 */
export async function runLanes(
  lanes: LaneSpec[],
  options: { jobs?: number; onStart?: (id: string) => void; onEnd?: (run: LaneRun) => void } = {}
): Promise<LaneRun[]> {
  const jobs = Math.max(1, options.jobs ?? UNLIMITED)
  const results = new Map<string, LaneRun>()
  const pending = [...lanes]
  const held = new Map<ResourceKey, ResourceClaim[`mode`][]>()
  const gates = new Map<string, `pending` | `open` | { failed: string }>()
  let running = 0

  return new Promise((resolve) => {
    const finish = (run: LaneRun) => {
      results.set(run.id, run)
      options.onEnd?.(run)
      if (results.size === lanes.length) {
        resolve(lanes.map((lane) => results.get(lane.id)!))
      }
    }

    const pump = () => {
      // A blocked lane still claims its keys, so nothing later overtakes it.
      const claimed = new Map(held)
      for (const lane of [...pending]) {
        const gate = gates.get(lane.id) ?? (lane.gate ? `pending` : `open`)
        if (typeof gate === `object`) {
          pending.splice(pending.indexOf(lane), 1)
          finish({ id: lane.id, ok: false, error: gate.failed, started: false, ms: 0 })
          continue
        }
        // Not ready yet: holds no claim (it might wait for minutes on the seed,
        // and the lanes behind it have nothing to do with that).
        if (gate === `pending`) continue
        const blocked = running >= jobs || collision(lane.resources, claimed) !== undefined
        hold(claimed, lane.resources)
        if (blocked) continue

        pending.splice(pending.indexOf(lane), 1)
        hold(held, lane.resources)
        running++
        options.onStart?.(lane.id)
        const started = Date.now()
        void lane
          .run()
          .then(
            () => ({ id: lane.id, ok: true, started: true, ms: Date.now() - started }),
            (error: unknown) => ({
              id: lane.id,
              ok: false,
              error: message(error),
              started: true,
              ms: Date.now() - started,
            })
          )
          .then((run: LaneRun) => {
            running--
            release(held, lane.resources)
            finish(run)
            pump()
          })
      }
    }

    if (lanes.length === 0) return resolve([])
    for (const lane of lanes) {
      if (!lane.gate) continue
      gates.set(lane.id, `pending`)
      lane.gate.then(
        () => {
          gates.set(lane.id, `open`)
          pump()
        },
        (error: unknown) => {
          gates.set(lane.id, { failed: `not started — ${lane.gateLabel ?? `its prerequisite`} failed: ${message(error)}` })
          pump()
        }
      )
    }
    pump()
  })
}

/* ---------------------------------------------------------------- the plan */

/**
 * The schedule as text: one row per lane in priority order, saying what it
 * waits for and why. Pure — what `--plan` prints and what every run prints
 * before the lanes start.
 */
export function describeSchedule(lanes: LaneSpec[], jobs: number): string[] {
  const lines = [
    `  jobs: ${jobs === UNLIMITED ? `unlimited (every lane that can overlap does)` : jobs === 1 ? `1 (--serial)` : jobs}`,
  ]
  const width = Math.max(...lanes.map((lane) => lane.id.length), 4) + 2
  // Only the keys that actually make some lane wait earn a "why" line.
  const contended = new Set<ResourceKey>()
  for (const [index, lane] of lanes.entries()) {
    const waits: string[] = []
    if (lane.gate || lane.gateLabel) waits.push(`after ${lane.gateLabel ?? `its gate`}`)
    for (const earlier of lanes.slice(0, index)) {
      const shared = lane.resources.find((claim) =>
        earlier.resources.some((other) => other.key === claim.key && conflicts(other.mode, claim.mode))
      )
      if (shared) {
        waits.push(`after ${earlier.id} (${shared.key})`)
        contended.add(shared.key)
      }
    }
    lines.push(
      `  ${lane.id.padEnd(width)}${lane.summary}` +
        `\n  ${` `.repeat(width)}→ ${waits.length === 0 ? `starts at once` : waits.join(` · `)}`
    )
  }
  if (contended.size > 0) {
    lines.push(`  why:`)
    for (const key of contended) lines.push(`    ${key.padEnd(15)}${RESOURCE_REASONS[key]}`)
  }
  return lines
}

/* ------------------------------------------------------------------- flags */

/**
 * `--serial` = 1, `--jobs N` = N, neither = unlimited. Throws on a `--jobs`
 * that is not a positive integer rather than silently running unlimited.
 */
export function parseJobs(argv: string[]): number {
  if (argv.includes(`--serial`)) return 1
  const index = argv.indexOf(`--jobs`)
  if (index === -1) return UNLIMITED
  const raw = argv[index + 1]
  const jobs = Number(raw)
  if (!raw || !Number.isInteger(jobs) || jobs < 1) {
    throw new Error(`--jobs needs a positive integer (got ${raw ?? `nothing`}); --serial = --jobs 1`)
  }
  return jobs
}

/* -------------------------------------------------------------- reruns */

/**
 * One `bun run shots` line per platform that failed views, narrowed to
 * exactly those views. `extra` carries flags the rerun must keep
 * (`--repos-root …`).
 */
export function rerunLines(failed: Map<string, string[]>, extra: string[] = []): string[] {
  const lines: string[] = []
  for (const [platform, views] of failed) {
    const unique = [...new Set(views)]
    if (unique.length === 0) continue
    lines.push(
      [`bun run shots --`, `--platform`, platform, `--views`, unique.join(`,`), ...extra].join(` `)
    )
  }
  return lines
}

/* ------------------------------------------------------------ log prefixes */

const laneLabel = new AsyncLocalStorage<string>()
let consolePatched = false

/**
 * Prefix every `console.*` line written inside `fn` (across awaits) with
 * `[label]`, so interleaved lanes stay readable. Lines that already carry a
 * `[…]` tag (`[desktop] board: captured`, a child's streamed output) are left
 * alone, and blank lines stay blank.
 */
export function withLaneLabel<T>(label: string, fn: () => Promise<T>): Promise<T> {
  patchConsole()
  return laneLabel.run(label, fn)
}

export function prefixLines(label: string, text: string): string {
  return text
    .split(`\n`)
    .map((line) => (line.trim() === `` || line.startsWith(`[`) ? line : `[${label}] ${line}`))
    .join(`\n`)
}

function patchConsole(): void {
  if (consolePatched) return
  consolePatched = true
  for (const method of [`log`, `info`, `warn`, `error`] as const) {
    const original = console[method].bind(console)
    console[method] = (...args: unknown[]) => {
      const label = laneLabel.getStore()
      if (!label) return original(...args)
      original(prefixLines(label, format(...args)))
    }
  }
}
