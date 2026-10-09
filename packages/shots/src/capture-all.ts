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
 *   3. Capture every lane CONCURRENTLY (EXP-1267, `lanes.ts`): web, desktop,
 *      iOS and android each wait on their own device, so they overlap, and only
 *      the pairs that truly share something (the screen-region desktop lane vs
 *      the device fleet other lanes photograph, one simulator, one emulator,
 *      one `capture-views.json`) are serialized — `--plan` prints the schedule
 *      and why. The backend-free `package` lanes start before the seed. A lane
 *      that FAILS does not stop the others: a broken emulator should still let
 *      the web and desktop lanes refresh, and the summary says exactly what is
 *      missing, with a rerun line per platform for just the failed views.
 *   4. Import the native lanes' output, write the store, rebuild the index.
 *   5. Summarise, and print `git status shots/` — the review-shaped answer to
 *      "what did this run actually change?".
 *
 * Everything long-lived is tracked and killed in a `finally`, so a Ctrl-C leaves
 * no orphan relay stub or desktop window behind.
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, join } from "node:path"
import {
  PLATFORMS,
  captureFor,
  viewsFor,
  type NativeCapture,
  type Platform,
} from "@exp/view-catalog"
import {
  FFI_ABIS,
  apkLibPath,
  apkListingHasFfi,
  appOnScreen,
  deviceFfiAbi,
  resolveAndroidSdk,
  specimenState,
  type AndroidSdk,
} from "./android.ts"
import { baselineSkipNote, scopeSince, storeBaseline, type AffectedScope } from "./affected.ts"
import {
  captureDesktop,
  mintSessionToken,
  resolveBinary,
  screenRecordingAllowed,
} from "./capture-desktop.ts"
import { fetchDemoIds, type DemoIds } from "./ids.ts"
import { importNative, NATIVE_PLATFORMS } from "./import-native.ts"
import {
  describeSchedule,
  parseJobs,
  rerunLines,
  runLanes,
  withLaneLabel,
  type LaneSpec,
  type ResourceClaim,
} from "./lanes.ts"
import { formatDuration, hasCommand, killChild, run, sleep, track, type Child } from "./lib/proc.ts"
import { rawDir, rawShotPath, repoRoot } from "./paths.ts"
import {
  formatDiffReport,
  indexStore,
  storeShotPath,
  toleranceFor,
  writeShot,
  type ShotDiffReport,
} from "./store.ts"

/** Through the Caddy h2 proxy — what the browser and the natives talk to. */
const PROXY_URL = `https://localhost:3000`
/** The vite dev server — what the desktop app talks to. */
const DEV_URL = `http://localhost:5173`
/** Compose services every capture depends on. */
const CORE_SERVICES = [`postgres`, `electric`, `caddy`]
/**
 * Views whose content only EXISTS when a desktop is online on the steer relay
 * (EXP-393): without one they render "Live steering is unavailable on this
 * instance", hide the Start-coding entry point entirely, or (the Agent
 * composer's chipped views, EXP-825) offer no machine to run on. `chat` itself
 * stays out: its sessions list and empty prompt render without a device.
 * `machine-settings` (EXP-733) opens the stub's OWN device row (`$device`) —
 * captured without the stub it has no row to open and skips outright.
 */
const STEER_DEPENDENT_VIEWS = new Set([
  // EXP-909: the Devices page anchors on the stand-in's device row and now
  // photographs the logins that row reports — nothing to frame without it.
  `agents`,
  `chat-issues`,
  `chat-action`,
  `steering`,
  `issue-detail`,
  `board`,
  `machine-settings`,
])
/**
 * Views that photograph the demo user's DEVICE FLEET: the Devices page, a
 * device's settings, the Add-server dialog, the wizard's devices step, and
 * every composer / runner picker that names a machine (EXP-1267). The desktop
 * lane registers its own machine for as long as it runs, so these must not be
 * photographed on another lane meanwhile — see `RESOURCE_REASONS.fleet`.
 * Conservative on purpose: a view wrongly listed here only costs overlap, a
 * view wrongly missing costs a ghost "Mac mini" in a committed shot.
 */
const FLEET_VIEWS = new Set([
  `agents`,
  `machine-settings`,
  `add-server`,
  `onboarding-devices`,
  `chat`,
  `chat-issues`,
  `chat-action`,
  `chat-from-issue`,
  `composer-menu`,
  `steering`,
  `action-page`,
  `action-create`,
  `action-triggers`,
  `trigger-editor`,
  `action-runs`,
  `settings-agents`,
  `settings-worktrees`,
  `getting-started`,
])
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
  /**
   * Narrow the run to the views a diff can actually have moved: everything
   * changed between this git ref and the working tree, mapped to views by
   * `affected.ts`. `auto` = the last commit that touched `shots/`, which is what
   * the unattended refresh automation runs after every merge.
   */
  since?: string
  /** EXP-1267: how many lanes may run at once (`--jobs N`; `--serial` = 1). */
  jobs: number
  /** Print the lane schedule for this scope and exit; touches nothing. */
  plan: boolean
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

  const sinceIndex = argv.indexOf(`--since`)
  const sinceValue = sinceIndex === -1 ? undefined : argv[sinceIndex + 1]
  const since =
    sinceIndex === -1 ? undefined : !sinceValue || sinceValue.startsWith(`--`) ? `auto` : sinceValue

  const requested = list(`platform`)
  const platforms = (requested ?? [...PLATFORMS]) as Platform[]
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
    since,
    jobs: parseJobs(argv),
    plan: argv.includes(`--plan`),
  }
}

/* ------------------------------------------------------------------ preflight */

interface Check {
  label: string
  ok: boolean
  detail?: string
}

/**
 * What each lane will actually photograph: the catalog's views for the platform,
 * narrowed by `--views` and by `--since`. Every consumer reads THIS rather than
 * the flags, so a lane whose set came out empty is skipped whole.
 */
type Scope = Map<Platform, Set<string>>

async function resolveScope(
  options: Options
): Promise<{ scope: Scope; affected?: AffectedScope; since?: string }> {
  let affected: AffectedScope | undefined
  let since: string | undefined
  if (options.since) {
    if (options.since === `auto`) {
      // EXP-667: the baseline is the last CAPTURE, not the last commit that
      // happened to touch shots/ — and when those differ, say so, because an
      // empty scope for the wrong reason looks just like a quiet product.
      const baseline = await storeBaseline()
      since = baseline?.ref
      if (baseline?.skipped) console.log(`shots: ${baselineSkipNote(baseline.skipped)}`)
    } else {
      since = options.since
    }
    if (!since) {
      throw new Error(
        `--since auto needs a baseline, but nothing has ever been committed under shots/`
      )
    }
    affected = await scopeSince(since, options.platforms)
  }

  const scope: Scope = new Map()
  for (const platform of options.platforms) {
    let ids = viewsFor(platform).map((view) => view.id)
    if (options.viewIds) ids = ids.filter((id) => options.viewIds!.includes(id))
    const narrowed = affected?.byPlatform.get(platform)
    if (narrowed) ids = ids.filter((id) => narrowed.includes(id))
    scope.set(platform, new Set(ids))
  }
  return { scope, affected, since }
}

/**
 * Drop `sign-in` from the browser and desktop lanes when this instance would
 * render a different card from the one the shot is about (EXP-642, EXP-812).
 *
 * The catalog's `sign-in` view is the CLOUD card: Google and Apple above the
 * email/password form, because that is what a new user actually meets. Those
 * buttons are env-driven (`buildAuthConfig`), so a dev instance without the
 * credentials renders a bare password box — a perfectly valid screen, and the
 * wrong one to commit under that name.
 *
 * The same payload answers the second question (EXP-812): whether the instance
 * advertises public SIGNUP. It does under `bun dev` and not in a production
 * build, which adds a "Create one" line and moves the card ~26px — a diff big
 * enough to be written on every run and reverted on every run.
 *
 * SOFT on purpose: the whole run is worth having without this one view, and a
 * capture host that cannot reach `/api/auth-config` (it is checked properly in
 * preflight a moment later) should not lose the view over it either. The native
 * lanes keep their `sg_sign-in` shot: since EXP-642 that name is the CLOUD
 * CHOOSER a first-run user meets, photographed before any instance is picked,
 * so this instance's posture cannot reach it.
 */
async function gateSignIn(scope: Scope): Promise<void> {
  const lanes: Platform[] = [`web`, `web-mobile`, `desktop`]
  if (!lanes.some((platform) => scope.get(platform)?.has(`sign-in`))) return

  let config:
    | {
        googleLoginEnabled?: boolean
        appleLoginEnabled?: boolean
        signupEnabled?: boolean
      }
    | undefined
  try {
    const response = await fetch(`${PROXY_URL}/api/auth-config`, {
      signal: AbortSignal.timeout(8_000),
      tls: { rejectUnauthorized: false },
    })
    if (response.ok) config = (await response.json()) as typeof config
  } catch {
    /* unreachable — preflight reports it; keep the view in scope */
  }
  if (!config) return

  const drop = (reason: readonly string[]): void => {
    for (const platform of lanes) scope.get(platform)?.delete(`sign-in`)
    console.log(
      [``, `── sign-in skipped ───────────────────────────────────────`, ...reason].join(`\n`)
    )
  }

  if (!config.googleLoginEnabled || !config.appleLoginEnabled) {
    drop([
      `  This instance advertises no Google/Apple sign-in, so the shot would be a bare`,
      `  password box rather than the cloud card the catalog describes. Export these`,
      `  (placeholder values are fine — nothing signs in through them) and re-run:`,
      `    GOOGLE_CLIENT_ID=… GOOGLE_CLIENT_SECRET=… GOOGLE_LOGIN_ENABLED=true`,
      `    APPLE_CLIENT_ID=… APPLE_CLIENT_SECRET=… APPLE_LOGIN_ENABLED=true`,
    ])
    return
  }

  // EXP-812: public signup is BUILD-derived (`lib/production-build.ts`), so the
  // dev server renders an extra "Don't have an account? Create one" line and
  // shifts the whole card up ~26px — 2.2% of sign-in/web and 6.6% of
  // sign-in/web-mobile, written every run and reverted every run. Gate on the
  // ADVERTISED flag rather than on dev-vs-built: a dev server with
  // AUTH_SIGNUP_ENABLED=false renders exactly the card the store holds.
  if (config.signupEnabled) {
    drop([
      `  This instance advertises public signup, so the card carries a "Create one" link`,
      `  the committed shot does not have. That is what a DEV server renders — auth`,
      `  posture is derived from the BUILD, not from NODE_ENV. Serve the built app:`,
      `    cd apps/web && bun run build`,
      `    PORT=5173 bun --env-file=.env .output/server/index.mjs`,
      `  (or set AUTH_SIGNUP_ENABLED=false on the server you are already running).`,
    ])
  }
}

/**
 * Drop the steering-dependent DESKTOP views when `--skip-relay` says no stub
 * will be online (EXP-732).
 *
 * The flag exists so a run that only wants the settings pages does not have to
 * stand up a relay — but the desktop lane launches the app either way, and
 * every view in [`STEER_DEPENDENT_VIEWS`] then photographs what the app falls
 * back to: a session tab stuck on Reconnecting (`steering` is a driven view
 * since EXP-732, not a hand capture), a board with no "coding now" badge, a
 * launcher with no machine to start on. A degraded shot under the right
 * filename is the one outcome the store cannot survive, so these are SKIPPED
 * the way the browser lane skips a view whose token could not be minted:
 * reported, not captured, never failed.
 *
 * Desktop only. The browser lane's steering-dependent views are the same
 * shots, but `--skip-relay` there has always been the flag people run while
 * iterating on one web view, and silently narrowing that is a bigger surprise
 * than the banner it prevents.
 */
function gateRelay(options: Options, scope: Scope): void {
  if (!options.skipRelay) return
  const dropped = laneViews(scope, `desktop`).filter((id) => STEER_DEPENDENT_VIEWS.has(id))
  if (dropped.length === 0) return
  for (const id of dropped) scope.get(`desktop`)?.delete(id)
  console.log(
    [
      ``,
      `── desktop: ${dropped.length} view(s) skipped (--skip-relay) ────────`,
      `  ${dropped.join(`, `)}`,
      `  Their content only exists while \`bun run screenshots:desktop\` has a machine online`,
      `  on the steer relay. Drop --skip-relay (and \`docker compose --profile steer up -d\`)`,
      `  to refresh them.`,
    ].join(`\n`)
  )
}

/**
 * Did anything narrow this run? Only then does a lane get an explicit view list.
 *
 * Derived from the SCOPE, not just from the flags: `gateSignIn` drops a view
 * without any flag being passed, and a lane that was handed no list captures
 * the whole catalog — which would photograph exactly the view the gate just
 * removed.
 */
/**
 * Does this run write every shot that differs at all, tolerance or not?
 *
 * Only when the operator named the views themselves (EXP-670). `--views` is
 * the deliberate, small, hand-reviewed set — the refresh automation's narrowed
 * lane, a re-run of one view — and there the tolerance is a liability: it is
 * tuned to absorb the seed's drifting relative timestamps, and a compact real
 * change (a queue reorder, a section that appeared) occupies about the same
 * pixel area. Twice in one run it kept a stale shot with only a percentage in
 * the log to hint at it.
 *
 * `--since` alone does NOT qualify: it routinely resolves to forty views, and
 * writing every timestamp flicker across all of them is the 200-file binary
 * diff the store writer exists to prevent. `--force` already writes
 * unconditionally, so it does not need this.
 */
function writesAnyChange(options: Options): boolean {
  return Boolean(options.viewIds) && !options.force
}

function isScoped(options: Options, scope: Scope): boolean {
  if (options.viewIds || options.since) return true
  return options.platforms.some(
    (platform) => (scope.get(platform)?.size ?? 0) !== viewsFor(platform).length
  )
}

/** The views one lane still has to capture, in catalog order. */
function laneViews(scope: Scope, ...platforms: Platform[]): string[] {
  const ids = new Set<string>()
  for (const platform of platforms) {
    for (const view of viewsFor(platform)) {
      if (scope.get(platform)?.has(view.id)) ids.add(view.id)
    }
  }
  return [...ids]
}

/**
 * Is the relay stub needed for this run?
 *
 * Any native platform (three of the eight store shots steer), or any in-scope
 * view whose content is steering-dependent. A web-only run of the settings
 * views does not need it, and making it unconditional would tax the fast path.
 */
function needsRelay(options: Options, scope: Scope): boolean {
  // A `package` capture (VAPP-88/89) is an SDK example app with no backend; only
  // the product app's fastlane lanes steer.
  if (
    options.platforms.some(
      (platform) =>
        NATIVE_PLATFORMS.includes(platform) &&
        laneViews(scope, platform).length > packageViews(scope, platform as `ios` | `android`).length
    )
  ) {
    return true
  }
  return laneViews(scope, ...options.platforms).some((id) => STEER_DEPENDENT_VIEWS.has(id))
}

async function composeServices(): Promise<{ states: Map<string, string>; problem?: string }> {
  const result = await run({
    cmd: [`docker`, `compose`, `ps`, `--format`, `json`],
    cwd: repoRoot(),
    // EXP-1267: a stopped Docker Desktop makes this hang, not fail.
    timeoutMs: 20_000,
  })
  const states = new Map<string, string>()
  if (result.code === 124) {
    return { states, problem: `\`docker compose ps\` did not answer within 20s — is Docker running?` }
  }
  if (result.code !== 0) {
    return { states, problem: `\`docker compose ps\` exited ${result.code}: ${result.stderr.trim().slice(-300)}` }
  }
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
  return { states }
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

async function preflight(options: Options, scope: Scope): Promise<Check[]> {
  const checks: Check[] = []
  // VAPP-88: a package-only run (the SDK example apps) photographs nothing
  // that talks to the backend; only the device tooling is checked.
  const backendless = packageOnly(scope)
  const compose = backendless ? { states: new Map<string, string>() } : await composeServices()
  const services = compose.states
  const running = (name: string): boolean => (services.get(name) ?? ``).toLowerCase() === `running`
  if (compose.problem) checks.push({ label: `docker compose ps`, ok: false, detail: compose.problem })

  for (const service of backendless ? [] : CORE_SERVICES) {
    checks.push({
      label: `docker compose: ${service}`,
      ok: running(service),
      detail: running(service)
        ? undefined
        : `not running — \`docker compose up -d\` (repo root)${options.up ? ` (--up tried and failed)` : ``}`,
    })
  }
  if (needsRelay(options, scope) && !options.skipRelay) {
    checks.push({
      label: `docker compose: steer-relay`,
      ok: running(`steer-relay`),
      detail: running(`steer-relay`)
        ? undefined
        : `not running — \`docker compose --profile steer up -d\` (needed by ${[...STEER_DEPENDENT_VIEWS].join(`, `)} and every native lane)`,
    })
  }

  if (!backendless) {
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
  }

  if (laneViews(scope, `web`, `web-mobile`).length > 0) {
    const script = webCaptureScript()
    checks.push({
      label: `apps/web: capture:views script`,
      ok: script,
      detail: script
        ? undefined
        : `missing from apps/web/package.json — the browser capturer is not landed yet`,
    })
  }

  if (laneViews(scope, `desktop`).length > 0) {
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

  if (laneViews(scope, `ios`).length > 0) {
    const ok = await hasCommand(`xcrun`)
    checks.push({
      label: `xcrun simctl`,
      ok,
      detail: ok ? undefined : `not on PATH — install Xcode and its command line tools`,
    })
  }

  if (packageViews(scope, `ios`).length > 0) {
    const ok = existsSync(join(repoRoot(), PACKAGE_IOS.project))
    checks.push({
      label: `iOS package example (${PACKAGE_IOS.project})`,
      ok,
      detail: ok ? undefined : `missing — the SwiftUI painter's example app has not landed`,
    })
  }

  if (packageViews(scope, `android`).length > 0) {
    const ok = existsSync(join(repoRoot(), PACKAGE_ANDROID.project, `example`))
    checks.push({
      label: `Android package example (${PACKAGE_ANDROID.project}/example)`,
      ok,
      detail: ok ? undefined : `missing — the Compose painter's example app has not landed`,
    })
    // EXP-1264: the lane builds the mobile FFI and the example with gradle; both
    // need the SDK (+ its NDK) and cargo.
    const sdk = androidSdk()
    checks.push({
      label: `Android SDK`,
      ok: sdk !== undefined,
      detail: sdk
        ? `${sdk.path} (${sdk.source}, exported as ANDROID_HOME)`
        : `not found — set sdk.dir in apps/android/local.properties, or ANDROID_HOME`,
    })
    const ndk = sdk !== undefined && (process.env.ANDROID_NDK_HOME !== undefined || existsSync(join(sdk.path, `ndk`)))
    checks.push({
      label: `Android NDK`,
      ok: ndk,
      detail: ndk ? undefined : `no ndk/ under the SDK — install one (SDK Manager → NDK) or set ANDROID_NDK_HOME`,
    })
    const cargo = await hasCommand(`cargo`)
    checks.push({
      label: `cargo (mobile FFI build)`,
      ok: cargo,
      detail: cargo ? undefined : `not on PATH — install rustup; the lane runs ${PACKAGE_ANDROID.ffiScript}`,
    })
  }

  if (laneViews(scope, `android`).length > 0) {
    const hasAdb = await hasCommand(`adb`)
    if (!hasAdb) {
      checks.push({ label: `adb`, ok: false, detail: `not on PATH — install the Android SDK` })
    } else {
      const devices = await run({ cmd: [`adb`, `devices`], timeoutMs: 15_000 })
      if (devices.code === 124) {
        checks.push({
          label: `adb: attached device`,
          ok: false,
          detail: `\`adb devices\` hung for 15s — \`adb kill-server && adb start-server\`, then re-run`,
        })
      }
      const attached = devices.stdout
        .split(`\n`)
        .slice(1)
        .filter((line) => /\tdevice$/.test(line.trim()))
      if (devices.code !== 124) checks.push({
        label: `adb: attached device`,
        ok: attached.length > 0,
        detail:
          attached.length > 0
            ? attached.length === 1
              ? undefined
              : `${attached.length} devices attached — screengrab needs exactly one`
            : `no booted emulator — start an English-locale phone emulator (\`emulator -avd ${ANDROID_AVD}\`)`,
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
  /**
   * EXP-1264: views this lane refused to photograph. The store counts them as
   * `failed` (not `missing`) so a crashed app is visible in the table.
   */
  failedViews?: string[]
  /**
   * EXP-1267: the lane never ran (its gate failed). Whatever `.shots-raw/`
   * holds for its views is a PREVIOUS run's, so the store must not encode it.
   */
  notRun?: boolean
}

/**
 * One `capture:views` pass over `views` for the given browser form factors.
 *
 * One browser run serves both form factors, so it captures the UNION and the
 * store writer drops what the narrower form factor did not ask for. The lane
 * builder may split the browser work into two passes (EXP-1267: the fleet
 * views wait for the desktop lane); passes never overlap, because both would
 * write the same `capture-views.json`.
 */
async function captureWebPass(
  formFactors: Platform[],
  views: string[],
  explicit: boolean,
  scope: Scope,
  outcomes: LaneOutcome[],
  label: string
): Promise<void> {
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
  if (explicit) cmd.push(`--views`, views.join(`,`))

  // EXP-913: the lane is judged on the results file it is about to write, so
  // the PREVIOUS run's rows must not be sitting there — a crash before the
  // first view would otherwise be read as "everything passed last time".
  // `capture:views` truncates it itself on entry; this is the belt for a lane
  // that never gets that far (or a `bun` that never starts).
  clearCaptureViewsResults()

  console.log(`\n── ${label} (${formFactor}, ${views.length} view(s)) ─────────────────────`)
  const result = await run({
    cmd,
    cwd: join(repoRoot(), `apps/web`),
    stream: true,
    label: `[${label}]`,
    timeoutMs: 30 * 60_000,
  })
  // EXP-913: capture:views records every view and keeps going, rewriting
  // `capture-views.json` after each one — so the verdict is per FORM FACTOR (a
  // web-mobile failure no longer fails web) and names the failed views, and a
  // run killed by the timeout still reports what it finished.
  const recorded = readCaptureViewsResults()
  const anyRecordedFailure = recorded?.some((row) => row.error !== undefined) ?? false
  for (const platform of formFactors) {
    const owed = views.filter((id) => scope.get(platform)?.has(id))
    if (owed.length === 0) continue
    const killed = result.code === 124 ? `capture:views timed out` : undefined
    if (!recorded || (result.code !== 0 && !killed && !anyRecordedFailure)) {
      // No per-view record, or an exit the record does not explain (a crash
      // before or between views): fall back to the exit code.
      outcomes.push({
        platform,
        ok: result.code === 0,
        detail: result.code === 0 ? undefined : `capture:views exited ${result.code}`,
        failedViews: result.code === 0 ? undefined : owed,
      })
      continue
    }
    const rows = recorded.filter((row) => row.formFactor === platform)
    const failed = rows.filter((row) => row.error !== undefined).map((row) => row.viewId)
    // A killed pass also owes every view it never reached.
    const unreached = killed ? owed.filter((id) => !rows.some((row) => row.viewId === id)) : []
    const problems = [
      ...(failed.length > 0 ? [`${failed.length} view(s) failed: ${failed.join(`, `)}`] : []),
      ...(killed ? [`${killed}${unreached.length > 0 ? ` before ${unreached.length} view(s)` : ``}`] : []),
    ]
    outcomes.push({
      platform,
      ok: problems.length === 0,
      detail: problems.length === 0 ? undefined : problems.join(` · `),
      failedViews: [...failed, ...unreached],
    })
  }
}

/** Drop the previous run's `capture-views.json` (best effort: an unwritable
 *  raw dir is the capture's problem to report, not this one's). */
function clearCaptureViewsResults(): void {
  const path = join(rawDir(), `capture-views.json`)
  try {
    rmSync(path, { force: true })
  } catch {
    // Nothing to do — the reader treats a missing OR unreadable file the same.
  }
}

/** `capture-views.json` (apps/web/scripts/capture-views.ts), or undefined. */
function readCaptureViewsResults():
  | { formFactor: string; viewId: string; error?: string }[]
  | undefined {
  const path = join(rawDir(), `capture-views.json`)
  if (!existsSync(path)) return undefined
  try {
    return JSON.parse(readFileSync(path, `utf8`))
  } catch {
    return undefined
  }
}

/** The shot ids one fastlane lane still owes. */
function laneShotIds(
  scope: Scope,
  platform: `ios` | `android`,
  lane: NativeCapture[`lane`]
): string[] {
  const ids = new Set<string>()
  for (const view of viewsFor(platform)) {
    if (!scope.get(platform)?.has(view.id)) continue
    const capture = captureFor(view, platform) as NativeCapture | undefined
    if (capture?.lane === lane) ids.add(capture.shot)
  }
  return [...ids]
}

/* ------------------------------------------------------- package lane (ios) */

/**
 * VAPP-88: the SDK example app the `package` captures come from. A blank Xcode
 * app (no Tuist, no Exponential code) over the SwiftUI painter's xcframework;
 * `-shot <view-id>` selects the view it renders.
 */
const PACKAGE_IOS = {
  project: `packages/exponential-ui-swift/Example/KitchenSink.xcodeproj`,
  scheme: `KitchenSink`,
  derivedData: `packages/exponential-ui-swift/Example/build`,
  app: `packages/exponential-ui-swift/Example/build/Build/Products/Release-iphonesimulator/KitchenSink.app`,
  bundleId: `at.exponential.ui.kitchensink`,
  /** The Snapfile's phone, so the package shot frames like the fastlane ones. */
  simulator: `iPhone 17 Pro Max`,
  settleMs: 3_000,
} as const

/**
 * Is EVERY view in scope a `package` capture (the SDK example apps)? Such a
 * run drives no browser, desktop or fastlane lane, so it needs no backend:
 * no compose stack, no dev server, no seed, no relay, no demo ids.
 */
function packageOnly(scope: Scope): boolean {
  if (laneViews(scope, `web`, `web-mobile`, `desktop`).length > 0) return false
  let any = false
  for (const platform of [`ios`, `android`] as const) {
    const views = laneViews(scope, platform)
    if (views.length === 0) continue
    any = true
    const pkg = new Set(packageViews(scope, platform))
    if (views.some((id) => !pkg.has(id))) return false
  }
  return any
}

/** In-scope views whose capture on `platform` is the `package` lane. */
function packageViews(scope: Scope, platform: `ios` | `android`): string[] {
  return viewsFor(platform)
    .filter((view) => scope.get(platform)?.has(view.id))
    .filter((view) => (captureFor(view, platform) as NativeCapture | undefined)?.lane === `package`)
    .map((view) => view.id)
}

/** The UDID of the named simulator, preferring a booted one. */
async function simulatorUdid(name: string): Promise<string | undefined> {
  const list = await run({
    cmd: [`xcrun`, `simctl`, `list`, `devices`, `available`, `-j`],
    timeoutMs: 60_000,
  })
  if (list.code !== 0) return undefined
  try {
    const parsed = JSON.parse(list.stdout) as {
      devices: Record<string, { udid: string; name: string; state: string }[]>
    }
    const matches = Object.values(parsed.devices)
      .flat()
      .filter((device) => device.name === name)
    return (matches.find((device) => device.state === `Booted`) ?? matches[0])?.udid
  } catch {
    return undefined
  }
}

/**
 * Capture the iOS `package` views from the SDK example app: build it once,
 * boot + pin the simulator like the fastlane lane (`override_status_bar`,
 * dark mode), then per view launch `-shot <id>` and `simctl io screenshot`
 * STRAIGHT into `.shots-raw/ios/<view-id>.png` — no fastlane dir, so the
 * native importer never sees these.
 */
async function capturePackageIOS(views: string[], outcomes: LaneOutcome[]): Promise<void> {
  if (views.length === 0) return
  console.log(`\n── ios: package example (${views.length} view(s)) ──────────────`)
  const root = repoRoot()
  const fail = (detail: string) => outcomes.push({ platform: `ios`, ok: false, detail, failedViews: views })

  const build = await run({
    cmd: [
      `xcodebuild`,
      `-project`,
      PACKAGE_IOS.project,
      `-scheme`,
      PACKAGE_IOS.scheme,
      `-configuration`,
      `Release`,
      `-destination`,
      `platform=iOS Simulator,name=${PACKAGE_IOS.simulator}`,
      `-derivedDataPath`,
      PACKAGE_IOS.derivedData,
      `CODE_SIGNING_ALLOWED=NO`,
      `build`,
    ],
    cwd: root,
    stream: true,
    label: `[ios:package]`,
    timeoutMs: 45 * 60_000,
  })
  if (build.code !== 0) return void fail(`package example build exited ${build.code}`)
  const app = join(root, PACKAGE_IOS.app)
  if (!existsSync(app)) return void fail(`package example built, but ${PACKAGE_IOS.app} is missing`)

  const udid = await simulatorUdid(PACKAGE_IOS.simulator)
  if (!udid) return void fail(`no available simulator named ${PACKAGE_IOS.simulator}`)
  const simctl = (args: string[], timeoutMs = 120_000) =>
    run({ cmd: [`xcrun`, `simctl`, ...args], cwd: root, timeoutMs })

  // `boot` refuses an already-booted device; `bootstatus -b` is the real wait.
  // EXP-1267: bounded at 4 minutes (a warm boot takes seconds, a cold one
  // well under two), so a wedged CoreSimulator fails the lane, not the night.
  await simctl([`boot`, udid], 60_000)
  const booted = await simctl([`bootstatus`, udid, `-b`], 4 * 60_000)
  if (booted.code !== 0) {
    return void fail(
      booted.code === 124
        ? `simulator ${PACKAGE_IOS.simulator} did not finish booting within 4m — \`xcrun simctl shutdown ${udid}\` (or erase it) and re-run`
        : `simulator ${PACKAGE_IOS.simulator} did not boot (simctl bootstatus exited ${booted.code})`
    )
  }
  await simctl([`ui`, udid, `appearance`, `dark`])
  await simctl([
    `status_bar`,
    udid,
    `override`,
    `--time`,
    `9:41`,
    `--batteryState`,
    `charged`,
    `--batteryLevel`,
    `100`,
    `--wifiBars`,
    `3`,
    `--cellularBars`,
    `4`,
  ])
  const installed = await simctl([`install`, udid, app], 5 * 60_000)
  if (installed.code !== 0) return void fail(`simctl install exited ${installed.code}`)

  const failed: string[] = []
  for (const viewId of views) {
    const png = rawShotPath(`ios`, viewId)
    // A stale raw file would be stored as if this run had produced it.
    rmSync(png, { force: true })
    mkdirSync(dirname(png), { recursive: true })
    await simctl([`terminate`, udid, PACKAGE_IOS.bundleId])
    const launched = await simctl([`launch`, udid, PACKAGE_IOS.bundleId, `-shot`, viewId])
    if (launched.code !== 0) {
      failed.push(viewId)
      console.log(`  fail  ${viewId} — simctl launch exited ${launched.code}`)
      continue
    }
    await sleep(PACKAGE_IOS.settleMs)
    const shot = await simctl([`io`, udid, `screenshot`, png])
    if (shot.code !== 0 || !existsSync(png)) {
      failed.push(viewId)
      console.log(`  fail  ${viewId} — simctl io screenshot exited ${shot.code}`)
      continue
    }
    console.log(`  ok    ${viewId} → ${png}`)
  }
  await simctl([`terminate`, udid, PACKAGE_IOS.bundleId])
  outcomes.push({
    platform: `ios`,
    ok: failed.length === 0,
    detail: failed.length === 0 ? undefined : `${failed.length} package view(s) failed: ${failed.join(`, `)}`,
    failedViews: failed,
  })
}

/* --------------------------------------------------- package lane (android) */

/** The phone AVD the android lanes are captured on (the fastlane lane's too). */
const ANDROID_AVD = `Medium_Phone_API_36.0`

/**
 * VAPP-89: the Compose painter's example app, the android twin of
 * `PACKAGE_IOS`. A plain Gradle Android app (no Exponential code) over the
 * painter AAR; the release build is debug-signed + R8, so it installs as is.
 * The `shot` extra selects the view it renders; `theme`/`mode` pin the look.
 */
const PACKAGE_ANDROID = {
  project: `packages/exponential-ui-compose`,
  task: `:example:assembleRelease`,
  apk: `packages/exponential-ui-compose/example/build/outputs/apk/release/example-release.apk`,
  appId: `at.exponential.ui.kitchensink`,
  activity: `at.exponential.ui.kitchensink/.MainActivity`,
  /** EXP-1264: the FFI the AAR bundles, built per run for the device's ABI. */
  ffiScript: `apps/desktop/crates/exponential-ui-ffi/build-android.sh`,
  ffiJniLibs: `apps/desktop/crates/exponential-ui-ffi/out/jniLibs`,
  /** How long a launch may take to log its first layout pass. */
  readyTimeoutMs: 20_000,
  settleMs: 3_000,
} as const

/** The Android SDK the app lane's gradle sees (`apps/android/local.properties` first). */
function androidSdk(): AndroidSdk | undefined {
  const props = join(repoRoot(), `apps/android/local.properties`)
  return resolveAndroidSdk({
    localProperties: existsSync(props) ? readFileSync(props, `utf8`) : undefined,
    env: process.env,
    home: homedir(),
    exists: existsSync,
  })
}

/** Serials of the attached, booted adb devices. */
async function adbDevices(): Promise<string[]> {
  const devices = await run({ cmd: [`adb`, `devices`], timeoutMs: 30_000 })
  if (devices.code !== 0) return []
  return devices.stdout
    .split(`\n`)
    .slice(1)
    .map((line) => line.trim())
    .filter((line) => /\tdevice$/.test(line))
    .map((line) => line.split(`\t`)[0] ?? ``)
}

/**
 * Capture the android `package` views from the Compose example app:
 *
 * 1. resolve the Android SDK like the app lane (`apps/android/local.properties`
 *    `sdk.dir`, else `ANDROID_HOME`, else `~/Library/Android/sdk`) and export
 *    it as `ANDROID_HOME` — the example project has no `local.properties`;
 * 2. build the mobile FFI (`build-android.sh`, only the device's ABI, the
 *    committed Kotlin binding kept) into `$CARGO_TARGET_DIR`, and check the
 *    `.so` landed in the crate's `out/jniLibs/<abi>`;
 * 3. build the release apk once and check it CARRIES `lib/<abi>/libexponential_ui_ffi.so`
 *    (without it the app dies in `dlopen` on start);
 * 4. per view: launch `--es shot <id>`, wait for the app's first-layout log
 *    line, then require the app alive AND focused before and after
 *    `adb exec-out screencap -p` writes `.shots-raw/android/<view-id>.png`.
 *
 * Any refusal is a FAILED view (the store table's `failed`), never a stored
 * shot: the launcher behind a crashed app looks like a real change. The caller
 * pins the status bar (demo mode) around it.
 */
async function capturePackageAndroid(views: string[], outcomes: LaneOutcome[]): Promise<void> {
  if (views.length === 0) return
  console.log(`\n── android: package example (${views.length} view(s)) ──────────`)
  const root = repoRoot()
  const fail = (detail: string) => outcomes.push({ platform: `android`, ok: false, detail, failedViews: views })

  const devices = await adbDevices()
  if (devices.length === 0) {
    return void fail(`no booted emulator — start one with \`emulator -avd ${ANDROID_AVD}\``)
  }
  if (devices.length > 1) return void fail(`${devices.length} devices attached — the package lane needs exactly one`)

  const adb = (args: string[], timeoutMs = 120_000) => run({ cmd: [`adb`, ...args], cwd: root, timeoutMs })

  const sdk = androidSdk()
  if (!sdk) {
    return void fail(`no Android SDK — set sdk.dir in apps/android/local.properties or ANDROID_HOME`)
  }
  console.log(`  Android SDK: ${sdk.path} (${sdk.source})`)

  const abilist = await adb([`shell`, `getprop`, `ro.product.cpu.abilist`])
  const abi = deviceFfiAbi(abilist.stdout) ?? deviceFfiAbi((await adb([`shell`, `getprop`, `ro.product.cpu.abi`])).stdout)
  if (!abi) return void fail(`the device's ABIs (${abilist.stdout.trim() || `unknown`}) are none of ${FFI_ABIS.join(`, `)}`)

  const env = { ANDROID_HOME: sdk.path }
  const ffi = await run({
    cmd: [`bash`, PACKAGE_ANDROID.ffiScript],
    cwd: root,
    env: { ...env, ABIS: abi, SKIP_BINDINGS: `1` },
    stream: true,
    label: `[android:ffi]`,
    timeoutMs: 45 * 60_000,
  })
  if (ffi.code !== 0) return void fail(`mobile FFI build (${PACKAGE_ANDROID.ffiScript}) exited ${ffi.code}`)
  const so = join(root, PACKAGE_ANDROID.ffiJniLibs, abi, `libexponential_ui_ffi.so`)
  if (!existsSync(so) || statSync(so).size === 0) {
    return void fail(`mobile FFI built, but ${PACKAGE_ANDROID.ffiJniLibs}/${abi}/libexponential_ui_ffi.so is missing`)
  }

  const build = await run({
    cmd: [`./gradlew`, PACKAGE_ANDROID.task],
    cwd: join(root, PACKAGE_ANDROID.project),
    env,
    stream: true,
    label: `[android:package]`,
    timeoutMs: 45 * 60_000,
  })
  if (build.code !== 0) return void fail(`package example build exited ${build.code}`)
  const apk = join(root, PACKAGE_ANDROID.apk)
  if (!existsSync(apk)) return void fail(`package example built, but ${PACKAGE_ANDROID.apk} is missing`)
  const listing = await run({ cmd: [`unzip`, `-l`, apk], timeoutMs: 60_000 })
  if (!apkListingHasFfi(listing.stdout, abi)) {
    return void fail(`${PACKAGE_ANDROID.apk} carries no ${apkLibPath(abi)} — the app would crash on start`)
  }

  const installed = await adb([`install`, `-r`, apk], 5 * 60_000)
  if (installed.code !== 0) return void fail(`adb install exited ${installed.code}`)

  const onScreen = async () =>
    appOnScreen(PACKAGE_ANDROID.appId, {
      pidof: (await adb([`shell`, `pidof`, PACKAGE_ANDROID.appId], 30_000)).stdout,
      dumpsys: (await adb([`shell`, `dumpsys`, `window`], 30_000)).stdout,
    })

  const failed: string[] = []
  const refuse = (viewId: string, png: string, why: string) => {
    failed.push(viewId)
    rmSync(png, { force: true })
    console.log(`  fail  ${viewId} — ${why}`)
  }
  for (const viewId of views) {
    const png = rawShotPath(`android`, viewId)
    // A stale raw file would be stored as if this run had produced it.
    rmSync(png, { force: true })
    mkdirSync(dirname(png), { recursive: true })
    await adb([`shell`, `am`, `force-stop`, PACKAGE_ANDROID.appId])
    // The ready marker is read off logcat: the previous view's must not count.
    await adb([`logcat`, `-c`], 30_000)
    const launched = await adb([
      `shell`, `am`, `start`, `-W`, `-n`, PACKAGE_ANDROID.activity,
      `--es`, `shot`, viewId, `--es`, `theme`, `exponential`, `--es`, `mode`, `dark`,
    ])
    // `am start` exits 0 even when it cannot resolve the activity.
    if (launched.code !== 0 || /Error/.test(launched.stdout + launched.stderr)) {
      refuse(viewId, png, `am start: ${(launched.stdout + launched.stderr).trim()}`)
      continue
    }
    // EXP-1264: wait for the first layout pass (the `.so` loaded, the specimen
    // resolved); a dead process ends the wait early.
    let ready: { ready: boolean; error?: string } = { ready: false }
    let alive = true
    const deadline = Date.now() + PACKAGE_ANDROID.readyTimeoutMs
    while (Date.now() < deadline) {
      const log = await adb([`logcat`, `-d`, `-s`, `ExponentialUI:*`], 30_000)
      ready = specimenState(log.stdout, viewId)
      if (ready.ready || ready.error) break
      alive = (await adb([`shell`, `pidof`, PACKAGE_ANDROID.appId], 30_000)).stdout.trim() !== ``
      if (!alive) break
      await sleep(500)
    }
    if (ready.error) {
      refuse(viewId, png, ready.error)
      continue
    }
    if (!ready.ready) {
      const crash = await adb([`logcat`, `-d`, `-b`, `crash`], 30_000)
      const cause = /UnsatisfiedLinkError[^\n]*|dlopen failed[^\n]*/.exec(crash.stdout)?.[0]
      refuse(
        viewId,
        png,
        alive
          ? `no first-layout log line within ${PACKAGE_ANDROID.readyTimeoutMs / 1000}s`
          : `the app died on start${cause ? `: ${cause.trim()}` : ``}`
      )
      continue
    }
    await sleep(PACKAGE_ANDROID.settleMs)
    const before = await onScreen()
    if (!before.ok) {
      refuse(viewId, png, before.reason)
      continue
    }
    // Binary PNG on stdout: `run()` decodes text, so pipe straight into the file.
    const shot = Bun.spawn({
      cmd: [`adb`, `exec-out`, `screencap`, `-p`],
      cwd: root,
      stdout: Bun.file(png),
      stderr: `ignore`,
      stdin: `ignore`,
    })
    const code = await shot.exited
    if (code !== 0 || !existsSync(png) || statSync(png).size === 0) {
      refuse(viewId, png, `adb exec-out screencap exited ${code}`)
      continue
    }
    // A crash DURING the screencap would have photographed the launcher too.
    const after = await onScreen()
    if (!after.ok) {
      refuse(viewId, png, `after the screenshot: ${after.reason}`)
      continue
    }
    console.log(`  ok    ${viewId} → ${png}`)
  }
  await adb([`shell`, `am`, `force-stop`, PACKAGE_ANDROID.appId])
  outcomes.push({
    platform: `android`,
    ok: failed.length === 0,
    detail: failed.length === 0 ? undefined : `${failed.length} package view(s) failed: ${failed.join(`, `)}`,
    failedViews: failed,
  })
}

/**
 * Android's autofill service raises a SYSTEM "Save password to Google Password
 * Manager?" dialog the moment the styleguide lane submits the login form. It is
 * not the app's window, so `mCurrentFocus` leaves the activity and Espresso's
 * very next interaction dies — as a `RootViewWithoutFocusException`, or, if the
 * dialog happens to eat the frames the first Electric sync needed, as a
 * `ComposeTimeoutException` on an assertion nowhere near the cause (EXP-665).
 * Three consecutive lane failures on 2026-08-28 were all this one dialog.
 *
 * Disabling the autofill service for the run is the only reliable prevention:
 * nothing in the test can dismiss a window it cannot see. Returns the previous
 * value so the caller can put the device back the way it found it.
 */
async function disableAndroidAutofill(): Promise<string | undefined> {
  const current = await run({
    cmd: [`adb`, `shell`, `settings`, `get`, `secure`, `autofill_service`],
    timeoutMs: 30_000,
  })
  const previous = current.stdout.trim()
  if (previous === `` || previous === `null`) return undefined
  await run({
    cmd: [`adb`, `shell`, `settings`, `put`, `secure`, `autofill_service`, `null`],
    timeoutMs: 30_000,
  })
  console.log(`[android] autofill disabled for the run (was ${previous})`)
  return previous
}

/**
 * The clock iOS pins with `override_status_bar` (Snapfile) — android gets the
 * same one so the two mobile lanes read alike in the gallery.
 */
const ANDROID_STATUS_BAR_CLOCK = `0941`

/**
 * SystemUI demo mode: android's answer to iOS's `override_status_bar`.
 *
 * Android's status bar is the DEVICE's — real clock, real notification icons,
 * real signal and battery. iOS has been pinned to 9:41 since the lane existed,
 * android never was, so an emulator that happened to read 10:59 rewrote all
 * sixteen android shots whose top strip was their only delta (2026-09-01, and
 * every refresh before it). Nothing downstream can tell that apart from a real
 * change: the strip is inside the frame and a diff is a diff.
 *
 * Demo mode freezes exactly that strip and nothing else — the app's own pixels
 * are untouched — so the shot stops carrying a timestamp. Battery and signal
 * are pinned for the same reason (a draining emulator moves the battery glyph),
 * and notification icons are hidden because a VPN or update icon drifting in
 * from the host is not part of the product.
 *
 * Returns whether it was entered, so the caller only undoes what it did: the
 * setting is device-global and a run that never enabled it must not clear a
 * value the owner set by hand.
 */
async function enableAndroidDemoMode(): Promise<boolean> {
  const previous = await run({
    cmd: [`adb`, `shell`, `settings`, `get`, `global`, `sysui_demo_allowed`],
    timeoutMs: 30_000,
  })
  const allowed = await run({
    cmd: [`adb`, `shell`, `settings`, `put`, `global`, `sysui_demo_allowed`, `1`],
    timeoutMs: 30_000,
  })
  if (allowed.code !== 0) {
    console.log(`[android] could not enable demo mode — status bar stays the device's`)
    return false
  }
  // One broadcast per command, in this order: `enter` first, or the rest are
  // dropped.
  const commands = [
    [`command`, `enter`],
    [`command`, `clock`, `-e`, `hhmm`, ANDROID_STATUS_BAR_CLOCK],
    [`command`, `notifications`, `-e`, `visible`, `false`],
    [`command`, `network`, `-e`, `wifi`, `show`, `-e`, `level`, `4`],
    [`command`, `network`, `-e`, `mobile`, `show`, `-e`, `datatype`, `none`, `-e`, `level`, `4`],
    [`command`, `battery`, `-e`, `level`, `100`, `-e`, `plugged`, `false`],
  ]
  for (const args of commands) {
    await run({
      cmd: [`adb`, `shell`, `am`, `broadcast`, `-a`, `com.android.systemui.demo`, `-e`, ...args],
      timeoutMs: 30_000,
    })
  }
  console.log(
    `[android] status bar pinned to ${ANDROID_STATUS_BAR_CLOCK.slice(0, 2)}:` +
      `${ANDROID_STATUS_BAR_CLOCK.slice(2)} for the run` +
      (previous.stdout.trim() === `1` ? `` : ` (demo mode was off)`)
  )
  return true
}

/** Leave demo mode and hand the status bar back — best effort, like autofill. */
async function restoreAndroidStatusBar(entered: boolean): Promise<void> {
  if (!entered) return
  try {
    await run({
      cmd: [
        `adb`, `shell`, `am`, `broadcast`, `-a`, `com.android.systemui.demo`,
        `-e`, `command`, `exit`,
      ],
      timeoutMs: 30_000,
    })
    console.log(`[android] status bar restored`)
  } catch {
    console.log(`[android] could not leave demo mode — run \`adb shell am broadcast -a com.android.systemui.demo -e command exit\``)
  }
}

/** Put `autofill_service` back — best effort; a failed restore must not fail the run. */
async function restoreAndroidAutofill(previous: string | undefined): Promise<void> {
  if (previous === undefined) return
  try {
    await run({
      cmd: [`adb`, `shell`, `settings`, `put`, `secure`, `autofill_service`, previous],
      timeoutMs: 30_000,
    })
    console.log(`[android] autofill restored (${previous})`)
  } catch {
    console.log(`[android] could not restore autofill to ${previous} — set it back by hand`)
  }
}

/** The two fastlane lanes, and the catalog lane each one photographs. */
const FASTLANE_LANES = {
  // `screenshots` is the 8-shot store set the catalog's `store` captures name,
  // `styleguide_screenshots` the wider parity set.
  store: `screenshots`,
  styleguide: `styleguide_screenshots`,
} as const

/** The view ids (not shot ids) one fastlane lane owes, in catalog order. */
function laneViewIds(
  scope: Scope,
  platform: `ios` | `android`,
  lane: NativeCapture[`lane`]
): string[] {
  return viewsFor(platform)
    .filter((view) => scope.get(platform)?.has(view.id))
    .filter((view) => (captureFor(view, platform) as NativeCapture | undefined)?.lane === lane)
    .map((view) => view.id)
}

let iosGenerated: Promise<void> | undefined

/** `tuist generate` ONCE for both iOS fastlane lanes (they skip their own). */
function generateIos(): Promise<void> {
  iosGenerated ??= run({
    cmd: [`tuist`, `generate`, `--no-open`],
    cwd: join(repoRoot(), `apps/ios`),
    stream: true,
    label: `[ios:generate]`,
    timeoutMs: 10 * 60_000,
  }).then((result) => {
    if (result.code !== 0) throw new Error(`tuist generate exited ${result.code}`)
  })
  return iosGenerated
}

async function captureFastlane(
  platform: `ios` | `android`,
  lane: `store` | `styleguide`,
  outcomes: LaneOutcome[],
  scope: Scope,
  scoped: boolean
): Promise<void> {
  const dir = join(repoRoot(), platform === `android` ? `apps/android` : `apps/ios`)
  const fastlane = FASTLANE_LANES[lane]
  const shots = laneShotIds(scope, platform, lane)
  if (shots.length === 0) {
    console.log(`\n── ${platform}: fastlane ${fastlane} — skipped, no shot in scope`)
    return
  }
  // `shots:<ids>` is the suites' own allowlist (EXP-642): navigation still
  // runs, but a snapshot outside the list is not taken. A simulator lane is
  // minutes per shot, so an unattended refresh that only moved two screens
  // must not pay for forty.
  const cmd = [`bundle`, `exec`, `fastlane`, fastlane]
  if (scoped) cmd.push(`shots:${shots.join(`,`)}`)

  console.log(`\n── ${platform}: fastlane ${fastlane} ──────────────────────`)
  const result = await run({
    cmd,
    cwd: dir,
    // fastlane's `snapshot` (iOS) shells out to the `simctl` gem, which reads
    // `xcrun simctl list -j devicetypes` as US-ASCII when the process locale
    // isn't UTF-8. A device type whose bundle path carries a non-ASCII byte
    // (e.g. a stray "ʀ" in an "iPhone Xʀ" profile) then blows up JSON
    // parsing; the gem swallows that and returns a bare identifier string,
    // which fastlane calls `.name` on and crashes with a NoMethodError that
    // looks nothing like an encoding issue (EXP-644).
    // SNAPSHOT_SKIP_GENERATE: `generateIos` already ran `tuist generate` once;
    // two lanes regenerating the same workspace side by side would race.
    env: platform === `ios` ? { LC_ALL: `en_US.UTF-8`, LANG: `en_US.UTF-8`, SNAPSHOT_SKIP_GENERATE: `1` } : undefined,
    stream: true,
    label: `[${platform}:${lane}]`,
    timeoutMs: 90 * 60_000,
  })
  outcomes.push({
    platform,
    ok: result.code === 0,
    detail: result.code === 0 ? undefined : `fastlane ${fastlane} exited ${result.code}`,
    // The suite reports no per-shot verdict: a failed lane owes all its views
    // (the importer still picks up any shot it did write).
    failedViews: result.code === 0 ? undefined : laneViewIds(scope, platform, lane),
  })
}

/**
 * Wait for the attached emulator to finish booting (EXP-1267). `adb devices`
 * lists an emulator as `device` well before Android is usable; anything driven
 * before `sys.boot_completed` fails in ways that look like app bugs. Bounded,
 * and says what to do when it runs out.
 */
async function waitForAndroidBoot(timeoutMs = 120_000): Promise<void> {
  const started = Date.now()
  let last = ``
  let announced = false
  while (Date.now() - started < timeoutMs) {
    const prop = await run({ cmd: [`adb`, `shell`, `getprop`, `sys.boot_completed`], timeoutMs: 15_000 })
    if (prop.code === 0 && prop.stdout.trim() === `1`) return
    last = prop.code === 124 ? `adb shell hung` : prop.stdout.trim() || prop.stderr.trim() || `empty`
    if (!announced) {
      console.log(`  waiting for the emulator to finish booting (up to ${formatDuration(timeoutMs)})…`)
      announced = true
    }
    await sleep(2_000)
  }
  throw new Error(
    `the emulator did not finish booting within ${formatDuration(timeoutMs)} (sys.boot_completed: ${last}) — ` +
      `cold-boot it (\`emulator -avd ${ANDROID_AVD} -no-snapshot-load\`) and re-run`
  )
}

/**
 * Start the stand-in desktop on the steer relay and wait for its banner.
 *
 * Waiting for the BANNER rather than a fixed sleep is the point: the stub only
 * prints it once both sockets are up and the `devices` row is registered, which
 * is exactly the condition the steering shots need. A capture that starts early
 * photographs "Reconnecting…".
 */
async function startRelayStub(): Promise<Child | undefined> {
  const secret = process.env.STEER_RELAY_SECRET ?? readEnvFile().STEER_RELAY_SECRET
  if (!secret) {
    throw new Error(
      `STEER_RELAY_SECRET is not set (checked the environment and the repo-root .env) — it must match the relay's. Use --skip-relay to run without steering-dependent views.`
    )
  }
  // Always localhost, on every lane (EXP-812). This URL is what the STUB dials
  // and what the web server advertises, and both live on this host; the ANDROID
  // emulator is the one client that cannot resolve it, so its suite rewrites the
  // authority of the minted dial URL to `10.0.2.2` on the device side instead
  // (SteerTestHooks + the `steerRelayUrl` launch argument). Pushing a LAN IP
  // through here was the old fix and it cost more than it bought: Chromium
  // blocks `ws://192.168.x.x` from the https capture page as mixed content, so
  // the browser and desktop lanes lost their steering shots and android had to
  // run in a pass of its own.
  const relayUrl = `ws://localhost:4002`

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

  const started = Date.now()
  const deadline = started + RELAY_TIMEOUT_MS
  let nextNotice = started + 15_000
  while (!online && Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`the relay stub exited (${child.exitCode}) before it came online`)
    }
    if (Date.now() >= nextNotice) {
      console.log(`[relay] still waiting for \`${RELAY_BANNER}\` (${formatDuration(Date.now() - started)} of ${formatDuration(RELAY_TIMEOUT_MS)})`)
      nextNotice += 15_000
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

/**
 * Can a sync client actually consume this instance's shape proxy?
 *
 * Every lane photographs an app whose content arrives over Electric, but NOTHING
 * downstream can tell "synced and genuinely empty" from "never synced": the
 * desktop app renders skeletons and definitive empty states either way, the
 * window is not flat, and `captureOne`'s variance gate is happy. A run against a
 * server whose shape responses a client cannot advance therefore produces a full
 * set of confidently-wrong screenshots and exits 0 — which is exactly what
 * happened, twice, and cost a whole store refresh.
 *
 * The failure mode that caused it is invisible from the body alone, and its
 * cause is two layers below anything this repo owns: on **node 26** a `fetch`
 * over a UNIX SOCKET returns the right status and the right body with ZERO
 * response headers (the same server over TCP is fine — it is an undici
 * regression). Nitro's dev worker is reached over exactly such a socket, so
 * `bun dev` serves every TanStack Start route with all of its headers gone: no
 * `content-type`, no `set-cookie`, and no `electric-handle`/`electric-offset`.
 * Those two ARE the shape cursor, so every shape re-requests `offset=-1`
 * forever, no collection ever reaches `up-to-date`, and the desktop app
 * photographs skeletons and "0 members in this team" while looking perfectly
 * alive. `.tool-versions` pins node 24.11.1 for this reason; nothing enforces
 * it, so a machine with a newer node on PATH breaks dev serving silently.
 *
 * So this asserts on the CONTRACT rather than on the data: the three headers
 * without which no client can sync, and the encoding sanity that a body claiming
 * to be plain must not actually be gzip. Cheap, one request, and it fires before
 * a single window is opened.
 */
async function assertShapesSyncable(baseUrl: string): Promise<void> {
  const fix =
    `The shape proxy is serving responses a sync client cannot advance.\n` +
    `  This is what \`bun dev\` does on machines where dev-mode serving drops route headers.\n` +
    `  Serve the built app instead (shots/README.md → Prerequisites):\n` +
    `    cd apps/web && bun run build && PORT=5173 bun --env-file=.env .output/server/index.mjs`

  let token: string
  try {
    token = await mintSessionToken(baseUrl)
  } catch (error) {
    throw new Error(
      `could not sign in at ${baseUrl} to check the shape proxy: ${error instanceof Error ? error.message : String(error)}`
    )
  }

  const response = await fetch(`${baseUrl}/api/shapes/boards?offset=-1`, {
    headers: { authorization: `Bearer ${token}`, origin: baseUrl },
    // Bun-only: the instance may sit behind Caddy's self-signed dev certificate.
    tls: { rejectUnauthorized: false },
    signal: AbortSignal.timeout(20_000),
  }).catch((error: unknown) => {
    throw new Error(
      `the boards shape at ${baseUrl} got no answer within 20s (${error instanceof Error ? error.message : String(error)}) — is Electric running?`
    )
  })
  if (!response.ok) {
    throw new Error(`the boards shape answered ${response.status} ${response.statusText}. ${fix}`)
  }

  // `electric-handle` + `electric-offset` ARE the resumption cursor; without them
  // a client re-requests offset=-1 forever and no shape ever reaches up-to-date.
  const missing = [`content-type`, `electric-handle`, `electric-offset`].filter(
    (header) => !response.headers.get(header)
  )
  if (missing.length > 0) {
    throw new Error(`the boards shape response is missing ${missing.join(`, `)}. ${fix}`)
  }

  // A gzip body that does not say so decodes to garbage in any client that takes
  // the header at its word — the desktop's ureq does.
  const body = new Uint8Array(await response.arrayBuffer())
  const gzipped = body[0] === 0x1f && body[1] === 0x8b
  if (gzipped && !response.headers.get(`content-encoding`)) {
    throw new Error(`the boards shape returned a gzip body with no content-encoding. ${fix}`)
  }
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
  options: Options,
  scope: Scope,
  outcomes: LaneOutcome[] = []
): Promise<{
  tallies: Map<Platform, PlatformTally>
  failures: string[]
  reports: ShotDiffReport[]
}> {
  const tallies = new Map<Platform, PlatformTally>()
  const failures: string[] = []
  const reports: ShotDiffReport[] = []
  // EXP-1264: a view a lane REFUSED (crashed app, not on screen) is a failure,
  // not merely a hole in the raw dir.
  const refused = new Set(
    outcomes.flatMap((outcome) => (outcome.failedViews ?? []).map((id) => `${outcome.platform}/${id}`))
  )
  const unrun = new Set(
    outcomes
      .filter((outcome) => outcome.notRun)
      .flatMap((outcome) => (outcome.failedViews ?? []).map((id) => `${outcome.platform}/${id}`))
  )

  for (const platform of options.platforms) {
    const tally = emptyTally()
    tallies.set(platform, tally)
    for (const view of viewsFor(platform)) {
      if (!scope.get(platform)?.has(view.id)) continue
      const raw = join(rawDir(), platform, `${view.id}.png`)
      if (unrun.has(`${platform}/${view.id}`)) {
        tally.failed++
        continue
      }
      if (!existsSync(raw)) {
        if (refused.has(`${platform}/${view.id}`)) tally.failed++
        else tally.missing++
        continue
      }
      try {
        const before = existingBytes(view.id, platform)
        const result = await writeShot(view.id, platform, readFileSync(raw), {
          force: options.force,
          writeAnyChange: writesAnyChange(options),
          dryRun: options.dryRun,
        })
        tally[result.state]++
        tally.bytesBefore += before
        tally.bytesAfter += result.bytes
        reports.push({
          viewId: view.id,
          platform,
          state: result.state,
          changedRatio: result.changedRatio,
          tolerance: toleranceFor(view.id),
        })
      } catch (error) {
        tally.failed++
        failures.push(
          `${platform}/${view.id}: ${error instanceof Error ? error.message : String(error)}`
        )
      }
    }
  }
  return { tallies, failures, reports }
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

/** One line per widened path, however many rules pointed at it. */
function dedupeBroad(broad: AffectedScope[`broad`]): AffectedScope[`broad`] {
  const seen = new Set<string>()
  return broad.filter((entry) => {
    const key = `${entry.path}|${entry.platforms.join(`,`)}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

/* --------------------------------------------------------------- the lanes */

/**
 * One schedulable lane plus what it owes: the views per platform (for the
 * rerun line when it dies wholesale) and its own outcome list, so the summary
 * reads in lane order whatever order they finished in.
 */
interface PlannedLane {
  spec: LaneSpec
  views: Map<Platform, string[]>
  outcomes: LaneOutcome[]
}

/**
 * Turn the scope into the lane list `runLanes` schedules (EXP-1267).
 *
 * Declaration order = priority (FIFO on shared keys): the backend-free
 * package lanes first (they start during the seed), then desktop, so the
 * fleet-free work of every other lane overlaps it and only the fleet views
 * wait for it, then the browser and the fastlane lanes.
 *
 * `backend` is the shared world (seed → shape check → relay stub → demo ids):
 * every lane that talks to the backend waits on it, and fails unrun if it
 * fails. `ids` is read only after that gate opened.
 */
function buildLanes(
  options: Options,
  scope: Scope,
  backend: { gate?: Promise<unknown>; ids: () => DemoIds }
): PlannedLane[] {
  const lanes: PlannedLane[] = []
  const scoped = isScoped(options, scope)
  const add = (
    id: string,
    summary: string,
    views: Map<Platform, string[]>,
    resources: ResourceClaim[],
    usesBackend: boolean,
    body: (outcomes: LaneOutcome[]) => Promise<void>
  ) => {
    const outcomes: LaneOutcome[] = []
    lanes.push({
      views,
      outcomes,
      spec: {
        id,
        summary,
        resources,
        gate: usesBackend ? backend.gate : undefined,
        gateLabel: usesBackend ? `seed` : undefined,
        run: () => withLaneLabel(id, () => body(outcomes)),
      },
    })
  }
  const fleetClaim = (views: string[]): ResourceClaim[] =>
    views.some((id) => FLEET_VIEWS.has(id)) ? [{ key: `fleet`, mode: `shared` }] : []
  const count = (views: string[], platform: string) => `${platform} · ${views.length} view(s)`

  // VAPP-88/89: the SDK example apps. No backend, so no gate.
  const iosPackage = options.platforms.includes(`ios`) ? packageViews(scope, `ios`) : []
  if (iosPackage.length > 0) {
    add(`ios:package`, count(iosPackage, `ios package example`), new Map([[`ios`, iosPackage]]),
      [{ key: `ios-simulator`, mode: `exclusive` }], false,
      (outcomes) => capturePackageIOS(iosPackage, outcomes))
  }
  const androidPackage = options.platforms.includes(`android`) ? packageViews(scope, `android`) : []
  if (androidPackage.length > 0) {
    add(`android:package`, count(androidPackage, `android package example`), new Map([[`android`, androidPackage]]),
      [{ key: `adb-device`, mode: `exclusive` }], false,
      async (outcomes) => {
        let demoMode = false
        try {
          await waitForAndroidBoot()
          // Pin the status bar exactly like the fastlane lane's shots.
          demoMode = await enableAndroidDemoMode()
          await capturePackageAndroid(androidPackage, outcomes)
        } finally {
          await restoreAndroidStatusBar(demoMode)
        }
      })
  }

  const desktopViews = options.platforms.includes(`desktop`) ? laneViews(scope, `desktop`) : []
  if (desktopViews.length > 0) {
    add(`desktop`, count(desktopViews, `desktop`), new Map([[`desktop`, desktopViews]]),
      [{ key: `screen`, mode: `exclusive` }, { key: `fleet`, mode: `exclusive` }], true,
      async (outcomes) => {
        console.log(`\n── desktop ───────────────────────────────────────────`)
        const result = await captureDesktop({
          ids: backend.ids(),
          viewIds: scoped ? desktopViews : undefined,
          reposRoot: options.reposRoot,
        })
        const failed = result.shots.filter((shot) => shot.state === `failed`).map((shot) => shot.viewId)
        outcomes.push({
          platform: `desktop`,
          ok: result.failures === 0,
          detail: result.failures === 0 ? undefined : `${result.failures} view(s) failed: ${failed.join(`, `)}`,
          failedViews: failed,
        })
      })
  }

  const formFactors = options.platforms.filter((platform) => platform === `web` || platform === `web-mobile`)
  const webViews = laneViews(scope, ...formFactors)
  if (webViews.length > 0) {
    const perPlatform = (views: string[]) =>
      new Map(formFactors.map((platform) => [platform, views.filter((id) => scope.get(platform)?.has(id))]))
    const results: ResourceClaim = { key: `capture-views`, mode: `exclusive` }
    const fleet = webViews.filter((id) => FLEET_VIEWS.has(id))
    const rest = webViews.filter((id) => !FLEET_VIEWS.has(id))
    // Split the browser work only when it buys overlap: the fleet-free views
    // run beside the desktop lane, the fleet views after it. Serial, or with
    // no desktop lane, one pass is cheaper (one browser, one sign-in).
    if (options.jobs > 1 && desktopViews.length > 0 && fleet.length > 0 && rest.length > 0) {
      add(`web`, count(rest, formFactors.join(`+`)), perPlatform(rest), [results], true,
        (outcomes) => captureWebPass(formFactors, rest, true, scope, outcomes, `web`))
      add(`web:fleet`, `${count(fleet, formFactors.join(`+`))} (device-fleet views)`, perPlatform(fleet),
        [results, ...fleetClaim(fleet)], true,
        (outcomes) => captureWebPass(formFactors, fleet, true, scope, outcomes, `web:fleet`))
    } else {
      add(`web`, count(webViews, formFactors.join(`+`)), perPlatform(webViews),
        [results, ...fleetClaim(webViews)], true,
        (outcomes) => captureWebPass(formFactors, webViews, scoped, scope, outcomes, `web`))
    }
  }

  for (const platform of [`ios`, `android`] as const) {
    if (!options.platforms.includes(platform)) continue
    for (const lane of [`store`, `styleguide`] as const) {
      const views = laneViewIds(scope, platform, lane)
      if (views.length === 0) continue
      const device: ResourceClaim =
        platform === `ios` ? { key: `ios-${lane}-simulator`, mode: `exclusive` } : { key: `adb-device`, mode: `exclusive` }
      add(`${platform}:${lane}`, count(views, `${platform} fastlane ${FASTLANE_LANES[lane]}`), new Map([[platform, views]]),
        [device, ...fleetClaim(views)], true,
        async (outcomes) => {
          if (platform === `ios`) {
            await generateIos()
            return captureFastlane(platform, lane, outcomes, scope, scoped)
          }
          let autofill: string | undefined
          let demoMode = false
          try {
            await waitForAndroidBoot()
            autofill = await disableAndroidAutofill()
            demoMode = await enableAndroidDemoMode()
            await captureFastlane(platform, lane, outcomes, scope, scoped)
          } finally {
            await restoreAndroidStatusBar(demoMode)
            await restoreAndroidAutofill(autofill)
          }
        })
    }
  }
  return lanes
}

/**
 * Every failed lane's outcomes, with a lane-wide death (a throw, a gate that
 * never opened) spread over the views it owed.
 */
function collectOutcomes(
  planned: PlannedLane[],
  runs: { ok: boolean; started: boolean; error?: string }[]
): LaneOutcome[] {
  const outcomes: LaneOutcome[] = []
  for (const [index, lane] of planned.entries()) {
    const run = runs[index]
    outcomes.push(...lane.outcomes)
    if (run && !run.ok) {
      for (const [platform, views] of lane.views) {
        outcomes.push({
          platform,
          ok: false,
          detail: `${lane.spec.id}: ${run.error ?? `failed`}`,
          failedViews: views,
          notRun: !run.started,
        })
      }
    }
  }
  return outcomes
}

/** The failed views per platform, from every failed outcome. */
function failedByPlatform(outcomes: LaneOutcome[]): Map<string, string[]> {
  const failed = new Map<string, string[]>()
  for (const outcome of outcomes) {
    if (outcome.ok || outcome.platform === `native-import`) continue
    failed.set(outcome.platform, [...(failed.get(outcome.platform) ?? []), ...(outcome.failedViews ?? [])])
  }
  return failed
}

/* ---------------------------------------------------------------------- main */

async function main(): Promise<number> {
  const options = parseArgs(process.argv.slice(2))
  const { scope, affected, since } = await resolveScope(options)
  if (!options.writeOnly) {
    // `--plan` touches nothing, not even the auth-config probe.
    if (!options.plan) await gateSignIn(scope)
    gateRelay(options, scope)
  }
  const relayNeeded =
    needsRelay(options, scope) && !options.skipRelay && !options.dryRun && !options.writeOnly

  console.log(
    `shots: platforms ${options.platforms.join(`, `)}` +
      (options.viewIds ? ` · views ${options.viewIds.join(`, `)}` : ``) +
      (options.dryRun ? ` · DRY RUN (nothing is written)` : ``)
  )
  if (writesAnyChange(options)) {
    console.log(
      `  --views run: writing every shot that differs AT ALL — the tolerance only advises here (EXP-670).\n` +
        `  Expect timestamp churn among the results; eyeball each one and \`git checkout HEAD --\` what the diff cannot explain.`
    )
  }

  if (affected && since) {
    console.log(`\n── affected since ${since.slice(0, 8)} ──────────────────────`)
    console.log(
      `  ${affected.changed.length} changed file(s), ${affected.ignored.length} of them irrelevant`
    )
    for (const platform of options.platforms) {
      const views = [...(scope.get(platform) ?? [])]
      console.log(
        `  ${platform.padEnd(12)}${views.length === 0 ? `— nothing to capture` : `${views.length}/${viewsFor(platform).length}: ${views.join(`, `)}`}`
      )
    }
    for (const entry of dedupeBroad(affected.broad)) {
      console.log(`  widened by ${entry.path} → ${entry.platforms.join(`, `)} (${entry.why})`)
    }
    for (const drop of affected.contentIgnored) {
      console.log(`  dropped by content: ${drop.path} — ${drop.why}`)
    }
    // Nothing survived the diff: the merge that triggered this run cannot have
    // moved a pixel. Say so and stop BEFORE preflight — the whole point of
    // --since is that this path costs seconds, not a seeded stack.
    if (options.platforms.every((platform) => (scope.get(platform)?.size ?? 0) === 0)) {
      console.log(`\nNothing to capture — no view in scope. Store left untouched.`)
      return 0
    }
  }

  if (options.plan) {
    const planned = buildLanes(options, scope, { ids: () => ({}) as DemoIds })
    console.log(`\n── lane schedule (--plan: nothing is checked, seeded or captured) ──`)
    if (planned.length === 0) console.log(`  no lane in scope`)
    else for (const line of describeSchedule(planned.map((lane) => lane.spec), options.jobs)) console.log(line)
    return 0
  }

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
  const checks = options.writeOnly ? [] : await preflight(options, scope)
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
  // what `--dry-run` promises not to do. What it still does is print the lane
  // schedule, re-encode whatever `.shots-raw/` already holds and report what
  // the store WOULD say.
  const outcomes: LaneOutcome[] = []
  let relay: Child | undefined
  let ids: DemoIds | undefined

  async function prepareBackend(): Promise<void> {
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
        throw new Error(`seeding failed (exit ${seed.code}) — every lane would photograph the wrong data`)
      }
    }

    // Before anything is driven: a server whose shape responses cannot be
    // advanced yields a complete set of confidently-empty screenshots that
    // every downstream check accepts. Fail here instead, in one request.
    console.log(`\n── shape proxy ───────────────────────────────────────`)
    await assertShapesSyncable(DEV_URL)
    console.log(`  ok    ${DEV_URL}/api/shapes — control headers present, body decodable`)

    // The relay stub comes BEFORE the id lookup on purpose: the demo user's own
    // `devices` row is written by the stub as it announces itself, not by the
    // seed, and `screenshots:ids` can only report a row that already exists. Ask
    // first and `$device` is unresolvable on every freshly-seeded run — which
    // silently skipped `machine-settings` (the Device settings dialog) forever.
    // ONE stub serves every lane: the relay replays its log to each viewer.
    if (relayNeeded) relay = await startRelayStub()

    ids = await fetchDemoIds()
    console.log(
      `\nids: team ${ids.teamId} · ${Object.keys(ids.issues).length} issues${ids.supportToken ? `` : ` · NO reporter token (support-reporter will skip)`}${ids.deviceId ? `` : ` · NO device row (machine-settings will skip)`}${ids.steeredSessionId ? `` : ` · NO showcase session (steering will skip)`}${ids.runChangesSessionId ? `` : ` · NO run-changes session (run-changes will skip)`}`
    )
  }

  try {
    if (!options.writeOnly) {
      // The shared world every backend lane needs, built once: seed → shape
      // check → relay stub → demo ids. A deferred gate, so the backend-free
      // package lanes start building while the seed runs.
      let openGate: () => void = () => {}
      let failGate: (error: unknown) => void = () => {}
      const gate = new Promise<void>((resolve, reject) => {
        openGate = resolve
        failGate = reject
      })
      const planned = buildLanes(options, scope, {
        gate,
        ids: () => {
          if (!ids) throw new Error(`demo ids were never resolved`)
          return ids
        },
      })
      const needsBackend = planned.some((lane) => lane.spec.gate !== undefined)

      console.log(`\n── lane schedule ─────────────────────────────────────`)
      if (planned.length === 0) console.log(`  no lane in scope`)
      for (const line of describeSchedule(planned.map((lane) => lane.spec), options.jobs)) console.log(line)
      if (!needsBackend && planned.length > 0) {
        // VAPP-88/89: the SDK example apps need no backend.
        console.log(`  package-only run: no seed, no relay, no demo ids`)
      }

      if (!options.dryRun && planned.length > 0) {
        const started = Date.now()
        const lanesDone = runLanes(
          planned.map((lane) => lane.spec),
          {
            jobs: options.jobs,
            onStart: (id) => console.log(`\n▶ ${id} started`),
            onEnd: (run) =>
              console.log(`\n${run.ok ? `■` : `✗`} ${run.id} ${run.ok ? `done` : `FAILED`} in ${formatDuration(run.ms)}${run.error ? ` — ${run.error}` : ``}`),
          }
        )
        if (needsBackend) {
          try {
            await prepareBackend()
            openGate()
          } catch (error) {
            console.error(`\nshared setup failed: ${error instanceof Error ? error.message : String(error)}`)
            console.error(`Every backend lane is skipped; the package lanes still finish.`)
            failGate(error)
          }
        }
        const runs = await lanesDone
        outcomes.push(...collectOutcomes(planned, runs))
        console.log(`\n── lanes (${formatDuration(Date.now() - started)} wall clock) ───────────────────────`)
        for (const run of runs) {
          console.log(`  ${run.ok ? `ok  ` : `FAIL`}  ${run.id.padEnd(20)}${formatDuration(run.ms)}${run.error ? ` — ${run.error}` : ``}`)
        }
      }
    }

    const nativeInScope = options.platforms.filter(
      (platform) => NATIVE_PLATFORMS.includes(platform) && laneViews(scope, platform).length > 0
    )
    if (nativeInScope.length > 0) {
      const imported = importNative({
        platforms: nativeInScope,
        viewIds: isScoped(options, scope) ? laneViews(scope, ...nativeInScope) : undefined,
        dryRun: options.dryRun,
      })
      console.log(`\n── import native ─────────────────────────────────────`)
      console.log(`  ${imported.imported.length} capture(s) copied into ${rawDir()}`)
      for (const warning of imported.warnings) console.log(`  warn: ${warning}`)
      outcomes.push({ platform: `native-import`, ok: true })
    }

    console.log(`\n── store ─────────────────────────────────────────────`)
    const { tallies, failures, reports } = await writeStore(options, scope, outcomes)
    const index = await indexStore({ prune: options.prune, dryRun: options.dryRun })
    const delta = printTable(tallies)
    const diffLines = formatDiffReport(reports)
    if (diffLines.length > 0) {
      // EXP-658: a kept shot that differed is a decision, not a no-op — show
      // how close it came so a real change absorbed by the tolerance is seen.
      console.log(`\ndiff-skip detail (fraction of pixels changed; kept = under tolerance):`)
      for (const line of diffLines) console.log(line)
    }
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
      // EXP-1267: one line per platform, narrowed to exactly what failed.
      const reruns = rerunLines(
        failedByPlatform(laneFailures),
        options.reposRoot ? [`--repos-root`, options.reposRoot] : []
      )
      if (reruns.length > 0) {
        console.error(`\nrerun just the failed views:`)
        for (const line of reruns) console.error(`  ${line}`)
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
