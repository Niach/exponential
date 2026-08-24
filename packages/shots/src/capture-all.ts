/**
 * `bun run shots` — the one command that refreshes the screenshot store
 * (EXP-566).
 *
 * Four clients, six platforms, four capture technologies (Playwright, XCTest,
 * Espresso, `screencapture`), and a shared prerequisite stack that all of them
 * need in exactly the same state: the same seeded database, the same steer relay
 * with a desktop online, the same demo user. Run by hand, that is a runbook
 * nobody follows twice the same way, and the store ends up holding shots of
 * subtly different instances that cannot be compared — which is the one thing
 * the store exists to make possible.
 *
 * So this is the sequencer, and its shape is deliberate:
 *
 *   1. PREFLIGHT everything first, report ALL failures, then abort. A run that
 *      dies twenty minutes in because `adb` sees no device has wasted twenty
 *      minutes; every prerequisite is cheap to check and is checked up front.
 *   2. Seed, resolve ids, bring up the relay stub — the shared world.
 *   3. Capture platform by platform. A platform that FAILS does not stop the
 *      others: a broken emulator should still let the web and desktop lanes
 *      refresh, and the summary says exactly what is missing.
 *   4. Import the native lanes' output, write the store, rebuild the index.
 *   5. Summarise, and print `git status shots/` — the review-shaped answer to
 *      "what did this run actually change?".
 *
 * Everything long-lived is tracked and killed in a `finally`, so a Ctrl-C leaves
 * no orphan relay stub or desktop window behind.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { join } from "node:path"
import {
  DEFAULT_PLATFORMS,
  PLATFORMS,
  viewsFor,
  type Platform,
} from "@exp/view-catalog"
import { captureDesktop, resolveBinary, screenRecordingAllowed } from "./capture-desktop.ts"
import { fetchDemoIds, type DemoIds } from "./ids.ts"
import { importNative, NATIVE_PLATFORMS } from "./import-native.ts"
import { hasCommand, killChild, run, sleep, track, type Child } from "./lib/proc.ts"
import { rawDir, repoRoot } from "./paths.ts"
import { indexStore, storeShotPath, writeShot } from "./store.ts"

/** Through the Caddy h2 proxy — what the browser and the natives talk to. */
const PROXY_URL = `https://localhost:3000`
/** The vite dev server — what the desktop app talks to. */
const DEV_URL = `http://localhost:5173`
/** Compose services every capture depends on. */
const CORE_SERVICES = [`postgres`, `electric`, `caddy`]
/**
 * Views whose content only EXISTS when a desktop is online on the steer relay
 * (EXP-393): without one they render "Live steering is unavailable on this
 * instance" or hide the Start-coding entry point entirely.
 */
const STEER_DEPENDENT_VIEWS = new Set([`start-coding`, `steering`, `issue-detail`, `board`])
/** The stub's stdout banner (apps/web/scripts/screenshot-desktop.ts). */
const RELAY_BANNER = `Screenshot desktop online:`
const RELAY_TIMEOUT_MS = 90_000

interface Options {
  platforms: Platform[]
  viewIds?: string[]
  skipSeed: boolean
  skipRelay: boolean
  force: boolean
  dryRun: boolean
  prune: boolean
  up: boolean
  /**
   * Skip every capture lane and only re-encode what `.shots-raw/` already
   * holds. Unlike `--dry-run` this DOES write the store — it is the second half
   * of a run whose first half already happened (a lane re-run by hand, a
   * tolerance change, a catalog rename).
   */
  writeOnly: boolean
  /** `<repos_root>` for the desktop lane's repo-backed views. */
  reposRoot?: string
}

function parseArgs(argv: string[]): Options {
  const flag = (name: string): string | undefined => {
    const index = argv.indexOf(`--${name}`)
    return index === -1 ? undefined : argv[index + 1]
  }
  const list = (name: string): string[] | undefined =>
    flag(name)
      ?.split(`,`)
      .map((value) => value.trim())
      .filter(Boolean)

  const requested = list(`platform`)
  const platforms = (requested ?? [...DEFAULT_PLATFORMS]) as Platform[]
  const unknown = platforms.filter((platform) => !PLATFORMS.includes(platform))
  if (unknown.length > 0) {
    throw new Error(`unknown platform(s): ${unknown.join(`, `)} (known: ${PLATFORMS.join(`, `)})`)
  }
  return {
    platforms,
    viewIds: list(`views`),
    skipSeed: argv.includes(`--skip-seed`),
    skipRelay: argv.includes(`--skip-relay`),
    force: argv.includes(`--force`),
    dryRun: argv.includes(`--dry-run`),
    prune: argv.includes(`--prune`),
    up: argv.includes(`--up`),
    writeOnly: argv.includes(`--write-only`),
    reposRoot: flag(`repos-root`) ?? process.env.SHOTS_REPOS_ROOT,
  }
}

/* ------------------------------------------------------------------ preflight */

interface Check {
  label: string
  ok: boolean
  detail?: string
}

/**
 * Is the relay stub needed for this run?
 *
 * Any native platform (three of the eight store shots steer), or any in-scope
 * view whose content is steering-dependent. A web-only run of the settings
 * views does not need it, and making it unconditional would tax the fast path.
 */
function needsRelay(options: Options): boolean {
  if (options.platforms.some((platform) => NATIVE_PLATFORMS.includes(platform))) return true
  const inScope = new Set(
    options.platforms.flatMap((platform) =>
      viewsFor(platform)
        .map((view) => view.id)
        .filter((id) => !options.viewIds || options.viewIds.includes(id))
    )
  )
  return [...inScope].some((id) => STEER_DEPENDENT_VIEWS.has(id))
}

async function composeServices(): Promise<Map<string, string>> {
  const result = await run({
    cmd: [`docker`, `compose`, `ps`, `--format`, `json`],
    cwd: repoRoot(),
    timeoutMs: 60_000,
  })
  const states = new Map<string, string>()
  if (result.code !== 0) return states
  // `docker compose ps --format json` emits either one array or one object per
  // line depending on the compose version — handle both rather than pinning one.
  for (const line of result.stdout.split(`\n`)) {
    const trimmed = line.trim()
    if (!trimmed) continue
    try {
      const parsed = JSON.parse(trimmed) as
        | { Service?: string; State?: string }
        | { Service?: string; State?: string }[]
      for (const entry of Array.isArray(parsed) ? parsed : [parsed]) {
        if (entry.Service) states.set(entry.Service, entry.State ?? ``)
      }
    } catch {
      /* a non-JSON progress line */
    }
  }
  return states
}

async function reachable(url: string): Promise<boolean> {
  try {
    const response = await fetch(url, {
      method: `GET`,
      signal: AbortSignal.timeout(8_000),
      // Bun-only fetch option: Caddy serves a self-signed dev certificate.
      tls: { rejectUnauthorized: false },
    })
    // Any HTTP answer proves the server is up; the app 302s anonymous requests.
    return response.status > 0
  } catch {
    return false
  }
}

async function preflight(options: Options): Promise<Check[]> {
  const checks: Check[] = []
  const services = await composeServices()
  const running = (name: string): boolean => (services.get(name) ?? ``).toLowerCase() === `running`

  for (const service of CORE_SERVICES) {
    checks.push({
      label: `docker compose: ${service}`,
      ok: running(service),
      detail: running(service)
        ? undefined
        : `not running — \`docker compose up -d\` (repo root)${options.up ? ` (--up tried and failed)` : ``}`,
    })
  }
  if (needsRelay(options) && !options.skipRelay) {
    checks.push({
      label: `docker compose: steer-relay`,
      ok: running(`steer-relay`),
      detail: running(`steer-relay`)
        ? undefined
        : `not running — \`docker compose --profile steer up -d\` (needed by ${[...STEER_DEPENDENT_VIEWS].join(`, `)} and every native lane)`,
    })
  }

  const proxyOk = await reachable(PROXY_URL)
  checks.push({
    label: `web app: ${PROXY_URL}`,
    ok: proxyOk,
    detail: proxyOk ? undefined : `unreachable — is \`bun dev\` running and Caddy proxying it?`,
  })
  const devOk = await reachable(DEV_URL)
  checks.push({
    label: `dev server: ${DEV_URL}`,
    ok: devOk,
    detail: devOk ? undefined : `unreachable — \`bun dev\` (repo root)`,
  })

  if (options.platforms.some((platform) => platform === `web` || platform === `web-mobile`)) {
    const script = webCaptureScript()
    checks.push({
      label: `apps/web: capture:views script`,
      ok: script,
      detail: script
        ? undefined
        : `missing from apps/web/package.json — the browser capturer is not landed yet`,
    })
  }

  if (options.platforms.includes(`desktop`)) {
    const allowed = await screenRecordingAllowed()
    checks.push({
      label: `macOS screen recording`,
      ok: allowed.ok,
      detail: allowed.ok ? undefined : allowed.message,
    })
    let binaryDetail: string | undefined
    try {
      binaryDetail = resolveBinary()
    } catch (error) {
      binaryDetail = undefined
      checks.push({
        label: `desktop app binary`,
        ok: false,
        detail: error instanceof Error ? error.message : String(error),
      })
    }
    if (binaryDetail) checks.push({ label: `desktop app binary`, ok: true, detail: binaryDetail })
  }

  if (options.platforms.includes(`ios`)) {
    const ok = await hasCommand(`xcrun`)
    checks.push({
      label: `xcrun simctl`,
      ok,
      detail: ok ? undefined : `not on PATH — install Xcode and its command line tools`,
    })
  }

  if (options.platforms.includes(`android`)) {
    const hasAdb = await hasCommand(`adb`)
    if (!hasAdb) {
      checks.push({ label: `adb`, ok: false, detail: `not on PATH — install the Android SDK` })
    } else {
      const devices = await run({ cmd: [`adb`, `devices`], timeoutMs: 30_000 })
      const attached = devices.stdout
        .split(`\n`)
        .slice(1)
        .filter((line) => /\tdevice$/.test(line.trim()))
      checks.push({
        label: `adb: attached device`,
        ok: attached.length > 0,
        detail:
          attached.length > 0
            ? attached.length === 1
              ? undefined
              : `${attached.length} devices attached — screengrab needs exactly one`
            : `no booted emulator — start an English-locale phone emulator`,
      })
    }
  }

  return checks
}

/** Has the sibling browser capturer landed? */
function webCaptureScript(): boolean {
  try {
    const pkg = JSON.parse(
      readFileSync(join(repoRoot(), `apps/web/package.json`), `utf8`)
    ) as { scripts?: Record<string, string> }
    return Boolean(pkg.scripts?.[`capture:views`])
  } catch {
    return false
  }
}

/* --------------------------------------------------------------------- lanes */

interface LaneOutcome {
  platform: Platform | `native-import`
  ok: boolean
  detail?: string
}

async function captureWeb(
  platforms: Platform[],
  options: Options,
  outcomes: LaneOutcome[]
): Promise<void> {
  const formFactors = platforms.filter(
    (platform) => platform === `web` || platform === `web-mobile`
  )
  if (formFactors.length === 0) return
  const formFactor = formFactors.length === 2 ? `all` : formFactors[0]!
  const cmd = [
    `bun`,
    `run`,
    `capture:views`,
    `--`,
    `--form-factor`,
    formFactor,
    `--out`,
    rawDir(),
  ]
  if (options.viewIds) cmd.push(`--views`, options.viewIds.join(`,`))

  console.log(`\n── web (${formFactor}) ─────────────────────────────────`)
  const result = await run({
    cmd,
    cwd: join(repoRoot(), `apps/web`),
    stream: true,
    label: `[web]`,
    timeoutMs: 30 * 60_000,
  })
  for (const platform of formFactors) {
    outcomes.push({
      platform,
      ok: result.code === 0,
      detail: result.code === 0 ? undefined : `capture:views exited ${result.code}`,
    })
  }
}

async function captureFastlane(
  platform: Platform,
  outcomes: LaneOutcome[]
): Promise<void> {
  const dir = join(repoRoot(), platform === `android` ? `apps/android` : `apps/ios`)
  // Both lanes matter: `screenshots` is the 8-shot store set the catalog's
  // `store` captures name, `styleguide_screenshots` the wider parity set.
  for (const lane of [`screenshots`, `styleguide_screenshots`]) {
    console.log(`\n── ${platform}: fastlane ${lane} ──────────────────────`)
    const result = await run({
      cmd: [`bundle`, `exec`, `fastlane`, lane],
      cwd: dir,
      stream: true,
      label: `[${platform}]`,
      timeoutMs: 90 * 60_000,
    })
    outcomes.push({
      platform,
      ok: result.code === 0,
      detail: result.code === 0 ? undefined : `fastlane ${lane} exited ${result.code}`,
    })
  }
}

/**
 * Start the stand-in desktop on the steer relay and wait for its banner.
 *
 * Waiting for the BANNER rather than a fixed sleep is the point: the stub only
 * prints it once both sockets are up and the `devices` row is registered, which
 * is exactly the condition the steering shots need. A capture that starts early
 * photographs "Reconnecting…".
 */
async function startRelayStub(options: Options): Promise<Child | undefined> {
  const secret = process.env.STEER_RELAY_SECRET ?? readEnvFile().STEER_RELAY_SECRET
  if (!secret) {
    throw new Error(
      `STEER_RELAY_SECRET is not set (checked the environment and the repo-root .env) — it must match the relay's. Use --skip-relay to run without steering-dependent views.`
    )
  }
  // The Android emulator resolves neither `localhost` (that is the emulator) nor
  // `10.0.2.2` (which means nothing to the SERVER, and the server calls this URL
  // too for device presence). Only the host's LAN address works on both sides.
  let relayUrl = `ws://localhost:4002`
  if (options.platforms.includes(`android`)) {
    const ip = await run({ cmd: [`ipconfig`, `getifaddr`, `en0`], timeoutMs: 10_000 })
    const address = ip.stdout.trim()
    if (!address) {
      throw new Error(`android is in scope but \`ipconfig getifaddr en0\` found no LAN address`)
    }
    relayUrl = `ws://${address}:4002`
  }

  console.log(`\n── steer relay stub (${relayUrl}) ─────────────────────`)
  const child = track(
    Bun.spawn({
      cmd: [`bun`, `run`, `screenshots:desktop`],
      cwd: join(repoRoot(), `apps/web`),
      env: {
        ...process.env,
        STEER_RELAY_SECRET: secret,
        STEER_RELAY_URL: relayUrl,
      } as Record<string, string>,
      stdout: `pipe`,
      stderr: `pipe`,
      stdin: `ignore`,
    })
  )

  let online = false
  const decoder = new TextDecoder()
  const watch = (async () => {
    for await (const chunk of child.stdout as ReadableStream<Uint8Array>) {
      const text = decoder.decode(chunk, { stream: true })
      process.stdout.write(text.replace(/^/gm, `[relay] `))
      if (text.includes(RELAY_BANNER)) online = true
    }
  })()
  void watch
  // Drain stderr too — the stub writes heartbeat errors there, and an unread
  // pipe would deadlock it mid-run the moment the buffer fills, silently
  // turning every later steering shot into a "no desktop online" state.
  const errDecoder = new TextDecoder()
  const watchErr = (async () => {
    for await (const chunk of child.stderr as ReadableStream<Uint8Array>) {
      process.stderr.write(errDecoder.decode(chunk, { stream: true }).replace(/^/gm, `[relay!] `))
    }
  })()
  void watchErr

  const deadline = Date.now() + RELAY_TIMEOUT_MS
  while (!online && Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`the relay stub exited (${child.exitCode}) before it came online`)
    }
    await sleep(500)
  }
  if (!online) {
    killChild(child)
    throw new Error(
      `the relay stub never printed \`${RELAY_BANNER}\` within ${RELAY_TIMEOUT_MS / 1000}s — check STEER_RELAY_URL/SECRET against the running relay`
    )
  }
  return child
}

/** Minimal `.env` reader — only ever consulted for STEER_RELAY_SECRET. */
function readEnvFile(): Record<string, string> {
  const out: Record<string, string> = {}
  try {
    for (const line of readFileSync(join(repoRoot(), `.env`), `utf8`).split(`\n`)) {
      const match = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*)$/)
      if (!match) continue
      out[match[1]!] = match[2]!.trim().replace(/^["']|["']$/g, ``)
    }
  } catch {
    /* no .env is normal */
  }
  return out
}

/* --------------------------------------------------------------------- store */

interface PlatformTally {
  new: number
  updated: number
  kept: number
  failed: number
  missing: number
  bytesBefore: number
  bytesAfter: number
}

function emptyTally(): PlatformTally {
  return { new: 0, updated: 0, kept: 0, failed: 0, missing: 0, bytesBefore: 0, bytesAfter: 0 }
}

/**
 * Encode every raw capture the catalog claims into the store.
 *
 * Driven off the CATALOG, not off a directory walk: a stray PNG in
 * `.shots-raw/` (a hand-taken debug shot, a renamed view) must never end up
 * committed, and a view the catalog claims but no lane produced is reported as
 * `missing` rather than silently leaving the previous image in place unnoticed.
 */
async function writeStore(
  options: Options
): Promise<{ tallies: Map<Platform, PlatformTally>; failures: string[] }> {
  const tallies = new Map<Platform, PlatformTally>()
  const failures: string[] = []

  for (const platform of options.platforms) {
    const tally = emptyTally()
    tallies.set(platform, tally)
    for (const view of viewsFor(platform)) {
      if (options.viewIds && !options.viewIds.includes(view.id)) continue
      const raw = join(rawDir(), platform, `${view.id}.png`)
      if (!existsSync(raw)) {
        tally.missing++
        continue
      }
      try {
        const before = existingBytes(view.id, platform)
        const result = await writeShot(view.id, platform, readFileSync(raw), {
          force: options.force,
          dryRun: options.dryRun,
        })
        tally[result.state]++
        tally.bytesBefore += before
        tally.bytesAfter += result.bytes
      } catch (error) {
        tally.failed++
        failures.push(
          `${platform}/${view.id}: ${error instanceof Error ? error.message : String(error)}`
        )
      }
    }
  }
  return { tallies, failures }
}

function existingBytes(viewId: string, platform: Platform): number {
  try {
    // Through storeShotPath so the SHOTS_DIR override the writer honors also
    // governs the before/after byte deltas in the summary.
    return statSync(storeShotPath(viewId, platform)).size
  } catch {
    return 0
  }
}

/** Raw PNGs on disk that the catalog does not claim — a renamed view, usually. */
function strayRaw(): string[] {
  const stray: string[] = []
  for (const platform of PLATFORMS) {
    const dir = join(rawDir(), platform)
    if (!existsSync(dir)) continue
    const claimed = new Set(viewsFor(platform).map((view) => view.id))
    for (const name of readdirSync(dir)) {
      if (!name.endsWith(`.png`)) continue
      const viewId = name.slice(0, -`.png`.length)
      if (!claimed.has(viewId)) stray.push(`${platform}/${name}`)
    }
  }
  return stray
}

/* -------------------------------------------------------------------- report */

function formatBytes(bytes: number): string {
  const sign = bytes < 0 ? `-` : `+`
  const abs = Math.abs(bytes)
  if (abs < 1024) return `${sign}${abs} B`
  if (abs < 1024 * 1024) return `${sign}${(abs / 1024).toFixed(1)} KB`
  return `${sign}${(abs / (1024 * 1024)).toFixed(2)} MB`
}

function printTable(tallies: Map<Platform, PlatformTally>): number {
  const columns: (keyof PlatformTally)[] = [`new`, `updated`, `kept`, `failed`, `missing`]
  console.log(`\nplatform     ${columns.map((column) => column.padStart(8)).join(``)}    delta`)
  console.log(`${`─`.repeat(13 + columns.length * 8 + 12)}`)
  let delta = 0
  for (const [platform, tally] of tallies) {
    delta += tally.bytesAfter - tally.bytesBefore
    console.log(
      `${platform.padEnd(13)}${columns.map((column) => String(tally[column]).padStart(8)).join(``)}` +
        `    ${formatBytes(tally.bytesAfter - tally.bytesBefore)}`
    )
  }
  return delta
}

/* ---------------------------------------------------------------------- main */

async function main(): Promise<number> {
  const options = parseArgs(process.argv.slice(2))
  const relayNeeded =
    needsRelay(options) && !options.skipRelay && !options.dryRun && !options.writeOnly

  console.log(
    `shots: platforms ${options.platforms.join(`, `)}` +
      (options.viewIds ? ` · views ${options.viewIds.join(`, `)}` : ``) +
      (options.dryRun ? ` · DRY RUN (nothing is written)` : ``)
  )

  if (options.up && !options.dryRun) {
    console.log(`\n── docker compose --profile steer up -d ──────────────`)
    await run({
      cmd: [`docker`, `compose`, `--profile`, `steer`, `up`, `-d`],
      cwd: repoRoot(),
      stream: true,
      label: `[compose]`,
      timeoutMs: 10 * 60_000,
    })
  }

  // A write-only pass drives nothing, so it needs none of what preflight
  // guards: no stack, no simulators, no screen-recording grant. Only sharp,
  // which the encode would fail on loudly anyway.
  console.log(`\n── preflight ─────────────────────────────────────────`)
  const checks = options.writeOnly ? [] : await preflight(options)
  if (options.writeOnly) console.log(`  skipped — --write-only re-encodes .shots-raw/ only`)
  for (const check of checks) {
    console.log(`  ${check.ok ? `ok  ` : `FAIL`}  ${check.label}${check.detail ? `\n          ${check.detail.replace(/\n/g, `\n          `)}` : ``}`)
  }
  const failed = checks.filter((check) => !check.ok)
  if (failed.length > 0) {
    console.error(
      `\npreflight failed (${failed.length} of ${checks.length}): ${failed.map((check) => check.label).join(`, `)}`
    )
    console.error(`Fix the above and re-run. Nothing was captured or written.`)
    return 1
  }

  // A dry run stops here for the expensive half: seeding, driving four capture
  // technologies and standing up a relay all MUTATE the world, which is exactly
  // what `--dry-run` promises not to do. What it still does is re-encode
  // whatever `.shots-raw/` already holds and report what the store WOULD say.
  const outcomes: LaneOutcome[] = []
  let relay: Child | undefined
  let ids: DemoIds | undefined

  try {
    if (!options.dryRun && !options.writeOnly) {
      if (!options.skipSeed) {
        console.log(`\n── seed:screenshots ──────────────────────────────────`)
        const seed = await run({
          cmd: [`bun`, `run`, `seed:screenshots`],
          cwd: join(repoRoot(), `apps/web`),
          stream: true,
          label: `[seed]`,
          timeoutMs: 15 * 60_000,
        })
        if (seed.code !== 0) {
          console.error(`seeding failed (exit ${seed.code}) — every lane would photograph the wrong data.`)
          return 1
        }
      }

      ids = await fetchDemoIds()
      console.log(
        `\nids: team ${ids.teamId} · ${Object.keys(ids.issues).length} issues${ids.supportThreadId ? ` · support thread ${ids.supportThreadId}` : ` · NO support thread (support views will skip)`}`
      )

      if (relayNeeded) relay = await startRelayStub(options)

      await captureWeb(options.platforms, options, outcomes)

      if (options.platforms.includes(`desktop`)) {
        console.log(`\n── desktop ───────────────────────────────────────────`)
        try {
          const result = await captureDesktop({
            ids,
            viewIds: options.viewIds,
            reposRoot: options.reposRoot,
          })
          outcomes.push({
            platform: `desktop`,
            ok: result.failures === 0,
            detail: result.failures === 0 ? undefined : `${result.failures} view(s) failed`,
          })
        } catch (error) {
          outcomes.push({
            platform: `desktop`,
            ok: false,
            detail: error instanceof Error ? error.message : String(error),
          })
        }
      }

      for (const platform of [`ios`, `android`] as const) {
        if (!options.platforms.includes(platform)) continue
        try {
          await captureFastlane(platform, outcomes)
        } catch (error) {
          outcomes.push({
            platform,
            ok: false,
            detail: error instanceof Error ? error.message : String(error),
          })
        }
      }
    }

    const nativeInScope = options.platforms.filter((platform) =>
      NATIVE_PLATFORMS.includes(platform)
    )
    if (nativeInScope.length > 0) {
      const imported = importNative({
        platforms: nativeInScope,
        viewIds: options.viewIds,
        dryRun: options.dryRun,
      })
      console.log(`\n── import native ─────────────────────────────────────`)
      console.log(`  ${imported.imported.length} capture(s) copied into ${rawDir()}`)
      for (const warning of imported.warnings) console.log(`  warn: ${warning}`)
      outcomes.push({ platform: `native-import`, ok: true })
    }

    console.log(`\n── store ─────────────────────────────────────────────`)
    const { tallies, failures } = await writeStore(options)
    const index = await indexStore({ prune: options.prune, dryRun: options.dryRun })
    const delta = printTable(tallies)
    console.log(
      `\nindex.json: ${index.entries} entr${index.entries === 1 ? `y` : `ies`}, ${index.changed ? `rewritten` : `unchanged`}` +
        (index.orphans.length > 0
          ? ` · ${index.orphans.length} orphan(s)${options.prune ? ` pruned` : ` (run with --prune to delete)`}`
          : ``)
    )
    for (const orphan of index.orphans) {
      console.log(`  orphan: ${orphan.viewId}/${orphan.platform} — ${orphan.reason}`)
    }
    for (const stray of strayRaw()) console.log(`  stray raw capture (not stored): ${stray}`)
    console.log(`total store delta: ${formatBytes(delta)}`)
    for (const failure of failures) console.error(`  encode failed: ${failure}`)

    console.log(`\n── git status shots/ ─────────────────────────────────`)
    const status = await run({
      cmd: [`git`, `status`, `--porcelain`, `shots/`],
      cwd: repoRoot(),
      timeoutMs: 60_000,
    })
    const porcelain = status.stdout.trim()
    console.log(porcelain === `` ? `  (clean — nothing changed)` : porcelain)

    const laneFailures = outcomes.filter((outcome) => !outcome.ok)
    if (laneFailures.length > 0) {
      console.error(`\nlane failures:`)
      for (const failure of laneFailures) {
        console.error(`  ${failure.platform}: ${failure.detail ?? `failed`}`)
      }
    }
    return laneFailures.length > 0 || failures.length > 0 ? 1 : 0
  } finally {
    if (relay) killChild(relay)
  }
}

if (import.meta.main) {
  try {
    process.exit(await main())
  } catch (error) {
    console.error(`\nshots: ${error instanceof Error ? error.message : String(error)}`)
    process.exit(1)
  }
}
