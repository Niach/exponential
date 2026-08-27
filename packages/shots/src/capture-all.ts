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
  captureFor,
  viewsFor,
  type NativeCapture,
  type Platform,
} from "@exp/view-catalog"
import { lastStoreCommit, scopeSince, type AffectedScope } from "./affected.ts"
import {
  captureDesktop,
  mintSessionToken,
  resolveBinary,
  screenRecordingAllowed,
} from "./capture-desktop.ts"
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
  /**
   * Narrow the run to the views a diff can actually have moved: everything
   * changed between this git ref and the working tree, mapped to views by
   * `affected.ts`. `auto` = the last commit that touched `shots/`, which is what
   * the unattended refresh automation runs after every merge.
   */
  since?: string
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
    since,
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
    since = options.since === `auto` ? await lastStoreCommit() : options.since
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
 * Drop `sign-in` from the browser and desktop lanes when this instance is not
 * advertising the OIDC buttons the shot is about (EXP-642).
 *
 * The catalog's `sign-in` view is the CLOUD card: Google and Apple above the
 * email/password form, because that is what a new user actually meets. Those
 * buttons are env-driven (`buildAuthConfig`), so a dev instance without the
 * credentials renders a bare password box — a perfectly valid screen, and the
 * wrong one to commit under that name.
 *
 * SOFT on purpose: the whole run is worth having without this one view, and a
 * capture host that cannot reach `/api/auth-config` (it is checked properly in
 * preflight a moment later) should not lose the view over it either. The native
 * lanes keep their `sg_sign-in` shot — the simulators talk to the cloud
 * instance, not to this one.
 */
async function gateSignIn(scope: Scope): Promise<void> {
  const lanes: Platform[] = [`web`, `web-mobile`, `desktop`]
  if (!lanes.some((platform) => scope.get(platform)?.has(`sign-in`))) return

  let config: { googleLoginEnabled?: boolean; appleLoginEnabled?: boolean } | undefined
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
  if (config.googleLoginEnabled && config.appleLoginEnabled) return

  for (const platform of lanes) scope.get(platform)?.delete(`sign-in`)
  console.log(
    [
      ``,
      `── sign-in skipped ───────────────────────────────────────`,
      `  This instance advertises no Google/Apple sign-in, so the shot would be a bare`,
      `  password box rather than the cloud card the catalog describes. Export these`,
      `  (placeholder values are fine — nothing signs in through them) and re-run:`,
      `    GOOGLE_CLIENT_ID=… GOOGLE_CLIENT_SECRET=… GOOGLE_LOGIN_ENABLED=true`,
      `    APPLE_CLIENT_ID=… APPLE_CLIENT_SECRET=… APPLE_LOGIN_ENABLED=true`,
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
  if (options.platforms.some((platform) => NATIVE_PLATFORMS.includes(platform))) return true
  return laneViews(scope, ...options.platforms).some((id) => STEER_DEPENDENT_VIEWS.has(id))
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

async function preflight(options: Options, scope: Scope): Promise<Check[]> {
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
  if (needsRelay(options, scope) && !options.skipRelay) {
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

  // One simulator lane serves both frames — see `captureFastlane`.
  if (laneViews(scope, `ios`, `ipad`).length > 0) {
    const ok = await hasCommand(`xcrun`)
    checks.push({
      label: `xcrun simctl`,
      ok,
      detail: ok ? undefined : `not on PATH — install Xcode and its command line tools`,
    })
  }

  if (laneViews(scope, `android`).length > 0) {
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
  scope: Scope,
  outcomes: LaneOutcome[]
): Promise<void> {
  const formFactors = platforms.filter(
    (platform) => platform === `web` || platform === `web-mobile`
  )
  if (formFactors.length === 0) return
  // One browser run serves both form factors, so it captures the UNION and the
  // store writer drops what the narrower form factor did not ask for.
  const views = laneViews(scope, ...formFactors)
  if (views.length === 0) {
    console.log(`\n── web ───────────────────────────────────────────────\n  skipped — no view in scope`)
    return
  }
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
  if (isScoped(options, scope)) cmd.push(`--views`, views.join(`,`))

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

/**
 * The catalog platforms one fastlane directory photographs. `ipad` has no lane
 * of its own: the iOS Snapfile runs the store set on both simulators in one
 * pass and writes both into the same output dir.
 */
function servedBy(platform: `ios` | `android`): Platform[] {
  return platform === `ios` ? [`ios`, `ipad`] : [`android`]
}

/** The shot ids one fastlane lane still owes, deduped across served frames. */
function laneShotIds(
  scope: Scope,
  platform: `ios` | `android`,
  lane: NativeCapture[`lane`]
): string[] {
  const ids = new Set<string>()
  for (const served of servedBy(platform)) {
    for (const view of viewsFor(served)) {
      if (!scope.get(served)?.has(view.id)) continue
      const capture = captureFor(view, served) as NativeCapture | undefined
      if (capture?.lane === lane) ids.add(capture.shot)
    }
  }
  return [...ids]
}

async function captureFastlane(
  platform: `ios` | `android`,
  outcomes: LaneOutcome[],
  scope: Scope,
  scoped: boolean
): Promise<void> {
  const dir = join(repoRoot(), platform === `android` ? `apps/android` : `apps/ios`)
  // Both lanes matter: `screenshots` is the 8-shot store set the catalog's
  // `store` captures name, `styleguide_screenshots` the wider parity set.
  for (const lane of [`screenshots`, `styleguide_screenshots`] as const) {
    const shots = laneShotIds(scope, platform, lane === `screenshots` ? `store` : `styleguide`)
    if (shots.length === 0) {
      console.log(`\n── ${platform}: fastlane ${lane} — skipped, no shot in scope`)
      continue
    }
    // `shots:<ids>` is the suites' own allowlist (EXP-642): navigation still
    // runs, but a snapshot outside the list is not taken. A simulator lane is
    // minutes per shot, so an unattended refresh that only moved two screens
    // must not pay for forty.
    const cmd = [`bundle`, `exec`, `fastlane`, lane]
    if (scoped) cmd.push(`shots:${shots.join(`,`)}`)

    console.log(`\n── ${platform}: fastlane ${lane} ──────────────────────`)
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
      env: platform === `ios` ? { LC_ALL: `en_US.UTF-8`, LANG: `en_US.UTF-8` } : undefined,
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
  scope: Scope
): Promise<{ tallies: Map<Platform, PlatformTally>; failures: string[] }> {
  const tallies = new Map<Platform, PlatformTally>()
  const failures: string[] = []

  for (const platform of options.platforms) {
    const tally = emptyTally()
    tallies.set(platform, tally)
    for (const view of viewsFor(platform)) {
      if (!scope.get(platform)?.has(view.id)) continue
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

/* ---------------------------------------------------------------------- main */

async function main(): Promise<number> {
  const options = parseArgs(process.argv.slice(2))
  const { scope, affected, since } = await resolveScope(options)
  if (!options.writeOnly) await gateSignIn(scope)
  const relayNeeded =
    needsRelay(options, scope) && !options.skipRelay && !options.dryRun && !options.writeOnly

  console.log(
    `shots: platforms ${options.platforms.join(`, `)}` +
      (options.viewIds ? ` · views ${options.viewIds.join(`, `)}` : ``) +
      (options.dryRun ? ` · DRY RUN (nothing is written)` : ``)
  )

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
    // Nothing survived the diff: the merge that triggered this run cannot have
    // moved a pixel. Say so and stop BEFORE preflight — the whole point of
    // --since is that this path costs seconds, not a seeded stack.
    if (options.platforms.every((platform) => (scope.get(platform)?.size ?? 0) === 0)) {
      console.log(`\nNothing to capture — no view in scope. Store left untouched.`)
      return 0
    }
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
      if (relayNeeded) relay = await startRelayStub(options)

      ids = await fetchDemoIds()
      console.log(
        `\nids: team ${ids.teamId} · ${Object.keys(ids.issues).length} issues${ids.supportThreadId ? ` · support thread ${ids.supportThreadId}` : ` · NO support thread (support views will skip)`}${ids.deviceId ? `` : ` · NO device row (machine-settings will skip)`}`
      )

      await captureWeb(options.platforms, options, scope, outcomes)

      if (laneViews(scope, `desktop`).length > 0) {
        console.log(`\n── desktop ───────────────────────────────────────────`)
        try {
          const result = await captureDesktop({
            ids,
            viewIds: isScoped(options, scope) ? laneViews(scope, `desktop`) : undefined,
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
        if (laneViews(scope, ...servedBy(platform)).length === 0) continue
        try {
          await captureFastlane(platform, outcomes, scope, isScoped(options, scope))
        } catch (error) {
          outcomes.push({
            platform,
            ok: false,
            detail: error instanceof Error ? error.message : String(error),
          })
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
    const { tallies, failures } = await writeStore(options, scope)
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
