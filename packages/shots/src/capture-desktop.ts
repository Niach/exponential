/**
 * The desktop IDE capture lane (EXP-566). macOS only.
 *
 * The other three clients photograph themselves: Playwright drives the browser,
 * XCTest and Espresso drive the natives. The gpui desktop app has no UI-test
 * harness, and synthesising clicks into it is exactly the kind of flaky
 * machinery this store exists to avoid. So the app is driven the way it already
 * supports being driven — the DEV env vars in `src/desktop-env.md` — and the
 * screenshot is taken from OUTSIDE, by macOS:
 *
 *   1. mint a session token from the demo credentials (`EXP_DEV_TOKEN`),
 *   2. per view: the run's throwaway `EXP_DATA_DIR`, the drive vars, a pinned
 *      window size, spawn the release binary,
 *   3. poll System Events (by pid) until the app has a window — CGWindowList
 *      via JXA is a dead end on current macOS: `deepUnwrap` hands back an empty
 *      object and the CFBridging route segfaults, while the Accessibility query
 *      is stable,
 *   4. wait out the view's `anchorDelayMs` — Electric is still streaming,
 *   5. raise the window and `screencapture -x -R<x,y,w,h>` its rect,
 *   6. gate on luminance variance: a flat dark rectangle means the window was
 *      caught before first paint, so retry once with more settle time,
 *   7. kill the app; the data dir is wiped once per RUN, not per view.
 *
 * The throwaway `EXP_DATA_DIR` is not paranoia — the docs are explicit that
 * `EXP_DEV_SERVER`+`EXP_DEV_TOKEN` PERSIST the injected account, so a run
 * without it rewrites the developer's real signed-in state and remembered
 * window size. ONE dir per run (not per view) keeps the app's device identity
 * stable, so a full lane registers a single synced `devices` row instead of
 * twenty ghost machines named after the host.
 *
 * The app is never BUILT here: cargo builds take minutes and belong to the
 * developer's own loop, so a missing binary is a preflight failure with the
 * exact command to fix it.
 */
import { mkdtempSync, readFileSync, rmSync, existsSync, mkdirSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { viewsFor, type DesktopCapture, type DesktopDrive } from "@exp/view-catalog"
// The demo identity lives in ONE place (apps/web/scripts/screenshot-demo.ts) so
// the seed, the relay stub and every capture lane sign in as the same user. It
// is a dependency-free constants module; importing it beats a copy that drifts.
import { DEMO_EMAIL, DEMO_PASSWORD } from "../../../apps/web/scripts/screenshot-demo.ts"
import { luminanceVariance } from "./encode.ts"
import { missingPlaceholder, resolveDriveValue, type DemoIds } from "./ids.ts"
import { killChild, run, sleep, track } from "./lib/proc.ts"
import { rawShotPath, repoRoot } from "./paths.ts"

/** `[[bin]] name` of `apps/desktop/crates/app` — also the CGWindow owner name. */
export const DEFAULT_PROCESS_NAME = `exp-desktop`
export const DEFAULT_BINARY = `apps/desktop/target/release/${DEFAULT_PROCESS_NAME}`
/** What the desktop app talks to. The vite dev server, not the Caddy proxy. */
export const DEFAULT_BASE_URL = `http://localhost:5173`
/** `PLATFORM_FRAME.desktop` is 1800×1125 — this @2x source scales exactly onto it. */
export const WINDOW_SIZE = `1440x900`
/**
 * The lane's device row needs a demo-plausible name: the app labels its synced
 * `devices` row from the OS hostname, and `api::users::hostname()` reads the
 * HOSTNAME env var first — without this, every committed desktop shot of the
 * Agents view would carry the developer's real machine name.
 */
export const DEVICE_HOSTNAME = `Alex's Mac mini`
/** Electric is still streaming when the window appears; the catalog can raise this. */
export const DEFAULT_ANCHOR_DELAY_MS = 6_000
const WINDOW_TIMEOUT_MS = 60_000
const WINDOW_POLL_MS = 500
/** Mean per-channel stdev below this is an unpainted window, not a screen. */
const FLAT_IMAGE_STDEV = 6
const FLAT_RETRY_EXTRA_MS = 4_000


export interface CaptureDesktopOptions {
  ids: DemoIds
  /** Restrict to these view ids. */
  viewIds?: readonly string[]
  binary?: string
  processName?: string
  baseUrl?: string
  dryRun?: boolean
}

export interface DesktopShotResult {
  viewId: string
  state: `captured` | `skipped` | `failed`
  file?: string
  reason?: string
}

export interface CaptureDesktopResult {
  shots: DesktopShotResult[]
  failures: number
}

/**
 * Can this process take screenshots at all?
 *
 * macOS gates `screencapture` behind the Screen Recording permission, and a
 * DENIED capture still exits 0 — it just writes a picture of the desktop
 * wallpaper. Probing with a 1×1 grab and checking the file exists is the only
 * cheap signal, so this is checked once in preflight rather than discovered
 * forty screenshots later.
 */
export async function screenRecordingAllowed(): Promise<
  { ok: true } | { ok: false; message: string }
> {
  if (process.platform !== `darwin`) {
    return { ok: false, message: `desktop captures need macOS (screencapture + System Events)` }
  }
  const grantMessage = [
    `macOS Screen Recording is not granted to this terminal.`,
    `  Grant it in System Settings → Privacy & Security → Screen & System Audio Recording,`,
    `  tick the app running this command (Terminal / iTerm / your editor), then RESTART that app —`,
    `  the permission is only picked up on a fresh launch.`,
  ].join(`\n`)

  // The authoritative check is the TCC API itself: a permission-DENIED
  // \`screencapture\` still exits 0 and writes a wallpaper-only image, so a
  // capture probe passes exactly the case this preflight exists to catch.
  // Neither JXA nor a shipped helper can reach CGPreflightScreenCaptureAccess,
  // but the system python3 can dlopen CoreGraphics directly.
  const tcc = await run({
    cmd: [
      `python3`,
      `-c`,
      `import ctypes; cg = ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics'); print('granted' if cg.CGPreflightScreenCaptureAccess() else 'denied')`,
    ],
    timeoutMs: 15_000,
  })
  if (tcc.code === 0) {
    const verdict = tcc.stdout.trim()
    if (verdict === `granted`) return { ok: true }
    if (verdict === `denied`) return { ok: false, message: grantMessage }
  }

  // No python3 (or the dlopen failed): fall back to the capture probe, which
  // at least catches a hard failure — but it CANNOT distinguish "denied but
  // photographing wallpaper", so say so.
  const probe = join(mkdtempSync(join(tmpdir(), `exp-shots-probe-`)), `probe.png`)
  try {
    const result = await run({ cmd: [`screencapture`, `-x`, `-R0,0,1,1`, probe], timeoutMs: 15_000 })
    if (result.code === 0 && existsSync(probe)) {
      console.warn(
        `[desktop] warning: could not query the Screen Recording permission directly ` +
          `(python3 unavailable) — if every shot comes out as wallpaper, see the grant steps in shots/README.md`
      )
      return { ok: true }
    }
    return { ok: false, message: `screencapture failed (exit ${result.code}). ${grantMessage}` }
  } finally {
    rmSync(dirname(probe), { recursive: true, force: true })
  }
}

/**
 * Sign the demo user in and return a session token for `EXP_DEV_TOKEN`.
 *
 * Better Auth requires a same-origin `Origin` header on the sign-in endpoint,
 * and the capture host may be talking to the Caddy proxy over a self-signed
 * cert — hence the explicit header and the relaxed TLS.
 */
export async function mintSessionToken(baseUrl: string): Promise<string> {
  const response = await fetch(`${baseUrl}/api/auth/sign-in/email`, {
    method: `POST`,
    headers: { "content-type": `application/json`, origin: baseUrl },
    body: JSON.stringify({ email: DEMO_EMAIL, password: DEMO_PASSWORD }),
    // Bun-only fetch option; the demo instance may sit behind Caddy's
    // self-signed dev certificate.
    tls: { rejectUnauthorized: false },
  })
  if (!response.ok) {
    throw new Error(
      `sign-in failed (${response.status} ${response.statusText}) for ${DEMO_EMAIL} at ${baseUrl} — has \`bun run seed:screenshots\` been run against this instance?`
    )
  }
  const body = (await response.json()) as { token?: string }
  if (body.token) return body.token
  // Older Better Auth builds only set the cookie; the desktop accepts either.
  const cookie = response.headers.get(`set-cookie`) ?? ``
  const match = cookie.match(/better-auth\.session_token=([^;]+)/)
  if (match) return decodeURIComponent(match[1]!)
  throw new Error(`sign-in returned neither a token nor a session cookie`)
}

/** Translate a catalog drive into the app's dev env vars. */
export function driveEnv(
  drive: DesktopDrive,
  ids: DemoIds
): { env: Record<string, string> } | { skip: string } {
  if (drive.kind === `manual`) {
    return { skip: `no automated path — capture it with \`--manual <view-id>\`` }
  }
  const value = resolveDriveValue(drive.value, ids)
  if (value === undefined) {
    return { skip: `unresolved placeholder ${missingPlaceholder(drive.value, ids)} — reseed?` }
  }
  switch (drive.kind) {
    case `tool`:
      return { env: { EXP_DEV_TOOL: value } }
    case `settings`:
      return { env: { EXP_DEV_SCREEN: `settings`, EXP_DEV_SETTINGS: value } }
    case `screen`: {
      const env: Record<string, string> = { EXP_DEV_SCREEN: value }
      // Pair the rail with the centre view, the way the app opens them itself:
      // a PR diff with the Reviews list beside it, a thread with Support.
      if (value.startsWith(`pr:`)) env.EXP_DEV_TOOL = `reviews`
      if (value.startsWith(`support:`)) env.EXP_DEV_TOOL = `support`
      return { env }
    }
  }
}

/** Capture every desktop view the catalog drives automatically. */
export async function captureDesktop(
  options: CaptureDesktopOptions
): Promise<CaptureDesktopResult> {
  const baseUrl = options.baseUrl ?? DEFAULT_BASE_URL
  const processName = options.processName ?? DEFAULT_PROCESS_NAME
  const binary = resolveBinary(options.binary)
  const wanted = options.viewIds ? new Set(options.viewIds) : undefined
  const shots: DesktopShotResult[] = []

  const views = viewsFor(`desktop`).filter((view) => !wanted || wanted.has(view.id))
  if (views.length === 0) return { shots, failures: 0 }

  if (options.dryRun) {
    for (const view of views) {
      const drive = driveEnv((view.desktop as DesktopCapture).drive, options.ids)
      shots.push(
        `skip` in drive
          ? { viewId: view.id, state: `skipped`, reason: drive.skip }
          : { viewId: view.id, state: `skipped`, reason: `dry run: ${describe(drive.env)}` }
      )
    }
    return { shots, failures: 0 }
  }

  const token = await mintSessionToken(baseUrl)
  // One data dir per RUN: a stable device identity across the lane's launches.
  const dataDir = mkdtempSync(join(tmpdir(), `exp-shots-desktop-`))

  for (const view of views) {
    const drive = driveEnv((view.desktop as DesktopCapture).drive, options.ids)
    if (`skip` in drive) {
      shots.push({ viewId: view.id, state: `skipped`, reason: drive.skip })
      console.log(`[desktop] ${view.id}: skipped — ${drive.skip}`)
      continue
    }
    try {
      const file = await captureOne({
        viewId: view.id,
        binary,
        processName,
        baseUrl,
        token,
        teamId: options.ids.teamId,
        dataDir,
        driveVars: drive.env,
        anchorDelayMs: (view.desktop as DesktopCapture).anchorDelayMs ?? DEFAULT_ANCHOR_DELAY_MS,
      })
      shots.push({ viewId: view.id, state: `captured`, file })
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      shots.push({ viewId: view.id, state: `failed`, reason })
      console.error(`[desktop] ${view.id}: FAILED — ${reason}`)
    }
  }

  rmSync(dataDir, { recursive: true, force: true })
  return { shots, failures: shots.filter((shot) => shot.state === `failed`).length }
}

interface CaptureOneOptions {
  viewId: string
  binary: string
  processName: string
  baseUrl: string
  token: string
  teamId: string
  dataDir: string
  driveVars: Record<string, string>
  anchorDelayMs: number
}

async function captureOne(options: CaptureOneOptions): Promise<string> {
  const out = rawShotPath(`desktop`, options.viewId)
  mkdirSync(dirname(out), { recursive: true })

  for (let attempt = 0; attempt < 2; attempt++) {
    const child = track(
      Bun.spawn({
        cmd: [options.binary],
        env: {
          ...process.env,
          EXP_INSTANCE_URL: options.baseUrl,
          EXP_DEV_SERVER: options.baseUrl,
          EXP_DEV_TOKEN: options.token,
          EXP_DEV_TEAM: options.teamId,
          EXP_DATA_DIR: options.dataDir,
          EXP_SKIP_ONBOARDING: `1`,
          EXP_WINDOW_SIZE: WINDOW_SIZE,
          HOSTNAME: DEVICE_HOSTNAME,
          ...options.driveVars,
        } as Record<string, string>,
        // Never `pipe` without a reader: a chatty gpui build (wgpu warnings,
        // env_logger) fills the ~64KB kernel buffer and blocks before painting.
        stdout: `ignore`,
        stderr: `ignore`,
        stdin: `ignore`,
      })
    )

    try {
      const rect = await waitForWindowRect(child.pid)
      if (!rect) {
        throw new Error(
          `no window from pid ${child.pid} (\`${options.processName}\`) within ${WINDOW_TIMEOUT_MS}ms (${describe(options.driveVars)})`
        )
      }
      await sleep(options.anchorDelayMs + (attempt === 1 ? FLAT_RETRY_EXTRA_MS : 0))
      // Raise the window right before the shutter: `-R` photographs a screen
      // RECT, so anything overlapping it would end up in the store.
      await raiseWindow(child.pid)
      await sleep(300)

      const capture = await run({
        cmd: [`screencapture`, `-x`, `-R${rect}`, out],
        timeoutMs: 30_000,
      })
      if (capture.code !== 0 || !existsSync(out)) {
        throw new Error(`screencapture exited ${capture.code}: ${capture.stderr.trim()}`)
      }

      const variance = await luminanceVariance(readFileSync(out))
      if (variance < FLAT_IMAGE_STDEV) {
        if (attempt === 0) {
          console.warn(
            `[desktop] ${options.viewId}: flat image (stdev ${variance.toFixed(1)}) — retrying with +${FLAT_RETRY_EXTRA_MS}ms`
          )
          continue
        }
        throw new Error(
          `window never painted (stdev ${variance.toFixed(1)} after retry) — is the app crashing on boot? Run it by hand with the same env`
        )
      }
      console.log(`[desktop] ${options.viewId}: captured (${describe(options.driveVars)})`)
      return out
    } finally {
      killChild(child)
    }
  }
  throw new Error(`unreachable`)
}

/**
 * Poll System Events (Accessibility) until the process has a window, and return
 * its screen rect as the `x,y,w,h` string `screencapture -R` takes. Keyed by
 * PID, never by process name — the developer's own installed Exponential.app is
 * the same binary name, and photographing THAT window would leak their real
 * workspace into the store.
 */
async function waitForWindowRect(pid: number): Promise<string | undefined> {
  const deadline = Date.now() + WINDOW_TIMEOUT_MS
  while (Date.now() < deadline) {
    const result = await run({
      cmd: [
        `osascript`,
        `-e`,
        `tell application "System Events" to tell (first application process whose unix id is ${pid}) to get {position, size} of window 1`,
      ],
      timeoutMs: 10_000,
    })
    // "144, 109, 1440, 901" — position then size, comma-joined by osascript.
    const parts = result.stdout.trim().split(`,`).map((part) => Number.parseInt(part.trim(), 10))
    if (result.code === 0 && parts.length === 4 && parts.every(Number.isFinite)) {
      const [x, y, w, h] = parts as [number, number, number, number]
      if (w >= 200 && h >= 200) return `${x},${y},${w},${h}`
    }
    await sleep(WINDOW_POLL_MS)
  }
  return undefined
}

/** Bring the process's windows to the front so nothing overlaps the rect. */
async function raiseWindow(pid: number): Promise<void> {
  await run({
    cmd: [
      `osascript`,
      `-e`,
      `tell application "System Events" to set frontmost of (first application process whose unix id is ${pid}) to true`,
    ],
    timeoutMs: 10_000,
  })
}

/**
 * Capture whatever window is frontmost, after a countdown.
 *
 * The escape hatch for the `manual` drives — the instance picker and the sign-in
 * screen, which by definition cannot be reached with a session token injected.
 * Arrange the window, run this, and it files the shot under the right view id.
 */
export async function captureManual(
  viewId: string,
  options: { countdownMs?: number } = {}
): Promise<string> {
  const countdown = options.countdownMs ?? 3_000
  console.log(`[desktop] capturing \`${viewId}\` — bring the window to the front.`)
  for (let left = Math.ceil(countdown / 1000); left > 0; left--) {
    console.log(`  ${left}…`)
    await sleep(1_000)
  }
  const frontmost = await run({
    cmd: [
      `osascript`,
      `-e`,
      `tell application "System Events" to get {name, unix id} of first application process whose frontmost is true`,
    ],
    timeoutMs: 10_000,
  })
  const [owner, pidText] = frontmost.stdout.trim().split(`,`).map((part) => part.trim())
  const pid = Number.parseInt(pidText ?? ``, 10)
  if (!owner || !Number.isFinite(pid)) {
    throw new Error(`could not determine the frontmost application`)
  }
  const rect = await waitForWindowRect(pid)
  if (!rect) throw new Error(`\`${owner}\` has no capturable window`)

  const out = rawShotPath(`desktop`, viewId)
  mkdirSync(dirname(out), { recursive: true })
  const capture = await run({
    cmd: [`screencapture`, `-x`, `-R${rect}`, out],
    timeoutMs: 30_000,
  })
  if (capture.code !== 0 || !existsSync(out)) {
    throw new Error(`screencapture exited ${capture.code}: ${capture.stderr.trim()}`)
  }
  console.log(`[desktop] ${viewId}: captured from \`${owner}\` → ${out}`)
  return out
}

/**
 * Absolute path of the app binary, or a failure carrying the build command.
 *
 * `CARGO_TARGET_DIR` comes FIRST because this repo's dev machines export it
 * globally to share one artifact cache across worktrees — with it set, cargo
 * never writes `apps/desktop/target/` at all, and defaulting to that path finds
 * nothing on exactly the machines that have already built the app.
 */
export function resolveBinary(binary?: string): string {
  const targetDir = process.env.CARGO_TARGET_DIR
  const candidates = binary
    ? [binary.startsWith(`/`) ? binary : join(repoRoot(), binary)]
    : [
        ...(targetDir ? [join(targetDir, `release`, DEFAULT_PROCESS_NAME)] : []),
        join(repoRoot(), DEFAULT_BINARY),
        // `cargo build -p app` names the package, the [[bin]] names the file —
        // accept both spellings so a hand-built binary is still found.
        join(repoRoot(), `apps/desktop/target/release/app`),
      ]
  const found = candidates.find((candidate) => existsSync(candidate))
  if (found) return found
  throw new Error(
    [
      `desktop app binary not found (looked at ${candidates.join(`, `)}).`,
      `  Build it first: cd apps/desktop && cargo build --release -p app`,
    ].join(`\n`)
  )
}

function describe(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([key, value]) => `${key}=${value}`)
    .join(` `)
}

if (import.meta.main) {
  const argv = process.argv.slice(2)
  const flag = (name: string): string | undefined => {
    const index = argv.indexOf(`--${name}`)
    return index === -1 ? undefined : argv[index + 1]
  }

  const manual = flag(`manual`)
  if (manual) {
    await captureManual(manual)
  } else {
    const { fetchDemoIds } = await import(`./ids.ts`)
    const allowed = await screenRecordingAllowed()
    if (!allowed.ok) {
      console.error(allowed.message)
      process.exit(1)
    }
    const result = await captureDesktop({
      ids: await fetchDemoIds(),
      viewIds: flag(`views`)?.split(`,`).filter(Boolean),
      binary: flag(`app-binary`),
      processName: flag(`process-name`),
      baseUrl: flag(`base-url`),
      dryRun: argv.includes(`--dry-run`),
    })
    if (result.failures > 0) process.exit(1)
  }
}
