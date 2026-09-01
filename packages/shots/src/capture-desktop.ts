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
 *      window size, a fresh `EXP_DEV_READY_FILE`, spawn the release binary,
 *   3. poll System Events (by pid) until the app has a window — CGWindowList
 *      via JXA is a dead end on current macOS: `deepUnwrap` hands back an empty
 *      object and the CFBridging route segfaults, while the Accessibility query
 *      is stable,
 *   4. wait for the app to WRITE its ready file (EXP-633): every shape Live,
 *      the session synced, the requested dialog settled. This replaced a fixed
 *      six-second sleep that was simultaneously too long for a settings pane
 *      and too short for a cold Electric — the app now says when it is ready,
 *      and `anchorDelayMs` is only the last frame of layout on top,
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
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { viewsFor, type DesktopCapture, type DesktopDrive } from "@exp/view-catalog"
// The demo identity lives in ONE place (apps/web/scripts/screenshot-demo.ts) so
// the seed, the relay stub and every capture lane sign in as the same user. It
// is a dependency-free constants module; importing it beats a copy that drifts.
import {
  DEMO_EMAIL,
  DEMO_PASSWORD,
  NEWCOMER_EMAIL,
  NEWCOMER_PASSWORD,
} from "../../../apps/web/scripts/screenshot-demo.ts"
import { luminanceVariance } from "./encode.ts"
import { missingPlaceholder, resolveDriveValue, type DemoIds } from "./ids.ts"
import { killChild, run, sleep, track, type Child } from "./lib/proc.ts"
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
/**
 * The prompt the dev shell renders in the `terminal` view. `HOSTNAME` above is
 * not enough: the shell is a real login shell on a real PTY, so `%m` in the
 * user's own prompt resolves through `gethostname(2)` and ignores the env var
 * — the committed shot carried `<developer>@<their-macbook>` verbatim, the
 * same class of leak as the username-free `--repos-root` rule in
 * `shots/README.md`. Pinning it also makes the frame deterministic: whatever a
 * developer's `.zshrc` does (git status in the prompt, a greeting, a theme)
 * would otherwise land in the store.
 */
export const SHELL_PROMPT = `alex@mac %1~ %# `
/**
 * Quiet time AFTER the app signalled ready — the last frame of layout, not a
 * sync wait. It used to be 6s of blind sleep with the catalog pushing some
 * views to 2.5s on top; the ready handshake below replaced all of that, so the
 * default is now small on purpose (EXP-633).
 */
export const DEFAULT_ANCHOR_DELAY_MS = 900
const WINDOW_TIMEOUT_MS = 60_000
const WINDOW_POLL_MS = 500
/**
 * How long the app gets to reach a photographable state after its window
 * appears. Generous because it covers a COLD Electric: the first launch of a
 * run streams all 19 shapes from offset -1. Anything slower than this is not a
 * slow machine, it is a stack that will never come up (a stopped Electric, an
 * unseeded instance), and failing loudly beats a skeleton PNG.
 */
const READY_TIMEOUT_MS = 90_000
const READY_POLL_MS = 250
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
  /**
   * Directory to use as the app's repos-&-worktrees root, in the app's own
   * `<repos_root>/<owner>/<name>` layout. The repo-backed views (files,
   * source-control, terminal, the desktop launcher) read the clone straight off
   * disk, so pointing this at a prepared checkout is the whole difference
   * between a file tree and "This repository is not cloned yet."
   */
  reposRoot?: string
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
 * Sign a capture identity in and return a session token for `EXP_DEV_TOKEN`.
 *
 * Defaults to the demo user; the onboarding drives pass the NEWCOMER instead,
 * because the wizard only renders for an account with no team.
 *
 * Better Auth requires a same-origin `Origin` header on the sign-in endpoint,
 * and the capture host may be talking to the Caddy proxy over a self-signed
 * cert — hence the explicit header and the relaxed TLS.
 */
export async function mintSessionToken(
  baseUrl: string,
  email: string = DEMO_EMAIL,
  password: string = DEMO_PASSWORD
): Promise<string> {
  const response = await fetch(`${baseUrl}/api/auth/sign-in/email`, {
    method: `POST`,
    headers: { "content-type": `application/json`, origin: baseUrl },
    body: JSON.stringify({ email, password }),
    // Bun-only fetch option; the demo instance may sit behind Caddy's
    // self-signed dev certificate.
    tls: { rejectUnauthorized: false },
  })
  if (!response.ok) {
    throw new Error(
      `sign-in failed (${response.status} ${response.statusText}) for ${email} at ${baseUrl} — has \`bun run seed:screenshots\` been run against this instance?`
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

/**
 * Is `token` still a live session?
 *
 * The lane mints once and then launches the app dozens of times, so a reseed
 * midway through rotates the demo user out from under it — `dev_inject_session`
 * then fails its validation, the shell renders the login pane, and every
 * remaining view is photographed as a sign-in card under the right filename.
 * That failure is invisible in the output (the window appears, it is not flat,
 * the capture "succeeds"), which is exactly why it is checked explicitly.
 */
export async function sessionValid(baseUrl: string, token: string): Promise<boolean> {
  try {
    const response = await fetch(`${baseUrl}/api/auth/get-session`, {
      headers: { authorization: `Bearer ${token}`, origin: baseUrl },
      tls: { rejectUnauthorized: false },
    })
    if (!response.ok) return false
    const body = (await response.json()) as { user?: { id?: string } } | null
    return Boolean(body?.user?.id)
  } catch {
    return false
  }
}

/** Translate a catalog drive into the app's dev env vars. */
export function driveEnv(
  drive: DesktopDrive,
  ids: DemoIds
): { env: Record<string, string> } | { skip: string } {
  if (drive.kind === `manual`) {
    return { skip: `no automated path — capture it with \`--manual <view-id>\`` }
  }
  // Nothing to set: the ABSENCE of EXP_DEV_SERVER/EXP_DEV_TOKEN is what leaves
  // the app signed out, and `captureOne` gets there via `signedOut`.
  if (drive.kind === `login`) return { env: {} }
  // The wizard is not a screen the shell routes to — it renders INSTEAD of the
  // shell, for an account with no team. `captureOne` supplies the rest of that
  // shape (newcomer token, scratch data dir, no EXP_SKIP_ONBOARDING).
  if (drive.kind === `onboarding`) return { env: { EXP_DEV_ONBOARDING: drive.value } }
  const value = resolveDriveValue(drive.value, ids)
  if (value === undefined) {
    return { skip: `unresolved placeholder ${missingPlaceholder(drive.value, ids)} — reseed?` }
  }
  switch (drive.kind) {
    case `tool`:
      return { env: { EXP_DEV_TOOL: value } }
    case `settings`:
      return { env: { EXP_DEV_SCREEN: `settings`, EXP_DEV_SETTINGS: value } }
    case `dialog`:
      // Every desktop dialog is a real OS window centred over the opener
      // (EXP-284), so it lands inside the main window's rect and the existing
      // window capture picks it up unchanged. The catalog pairs the spec with
      // whatever rail/screen belongs behind it via `desktop.env`.
      return { env: { EXP_DEV_DIALOG: value } }
    case `screen`: {
      const env: Record<string, string> = { EXP_DEV_SCREEN: value }
      // Pair the rail with the centre view, the way the app opens them itself:
      // a thread with Support. (Reviews stopped being a rail tool in EXP-706 —
      // the PR diff needs no pairing anymore.)
      if (value.startsWith(`support:`)) env.EXP_DEV_TOOL = `support`
      return { env }
    }
  }
}

/**
 * Resolve `$placeholders` in a view's extra `desktop.env`.
 *
 * The same substitution the DRIVE gets, because the two are interchangeable in
 * practice: `board-empty` is `tool:board` plus `EXP_DEV_BOARD_ID=$emptyBoard`,
 * and an unresolved id there is exactly as silently wrong as one in the drive —
 * the app ignores the malformed value and photographs the DEFAULT board under
 * the empty board's filename.
 */
export function resolveEnv(
  env: Record<string, string>,
  ids: DemoIds
): { env: Record<string, string> } | { skip: string } {
  const out: Record<string, string> = {}
  for (const [key, raw] of Object.entries(env)) {
    const value = resolveDriveValue(raw, ids)
    if (value === undefined) {
      return { skip: `unresolved placeholder ${missingPlaceholder(raw, ids)} in ${key} — reseed?` }
    }
    out[key] = value
  }
  return { env: out }
}

/** Everything one view's launch adds to the environment, placeholders resolved. */
function resolveDesktopVars(
  desktop: DesktopCapture,
  ids: DemoIds
): { env: Record<string, string> } | { skip: string } {
  const drive = driveEnv(desktop.drive, ids)
  if (`skip` in drive) return drive
  const extra = resolveEnv(desktop.env ?? {}, ids)
  if (`skip` in extra) return extra
  return { env: { ...drive.env, ...extra.env } }
}

/**
 * Delete the `devices` rows this lane just registered.
 *
 * Every launch announces itself, and a fresh data dir means a fresh device id,
 * so one run adds one machine to the demo team PERMANENTLY. Two runs later the
 * Agents screen, the Add-server dialog and the launcher's device picker all
 * photograph a fleet of identical ghosts. Best-effort: a failure here costs the
 * next run a stray row, never this run's shots.
 */
async function pruneStrayDevices(): Promise<void> {
  try {
    const result = await run({
      cmd: [`bun`, `run`, `screenshots:prune-devices`],
      cwd: join(repoRoot(), `apps/web`),
      timeoutMs: 60_000,
    })
    const line = result.stdout.trim().split(`\n`).at(-1)
    if (line) console.log(`[desktop] ${line}`)
  } catch (error) {
    console.warn(
      `[desktop] could not prune stray device rows: ${error instanceof Error ? error.message : String(error)}`
    )
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
      const resolved = resolveDesktopVars(view.desktop as DesktopCapture, options.ids)
      shots.push(
        `skip` in resolved
          ? { viewId: view.id, state: `skipped`, reason: resolved.skip }
          : {
              viewId: view.id,
              state: `skipped`,
              reason: `dry run: ${describe(resolved.env)}`,
            }
      )
    }
    return { shots, failures: 0 }
  }

  // Before AND after: a crashed run leaves its registration behind, and the
  // fleet in every machines-band shot has to be the seed's plus exactly ONE
  // machine — the one this lane is running on.
  await pruneStrayDevices()

  // One data dir per RUN: a stable device identity across the lane's launches.
  const dataDir = makeDataDir(options.reposRoot)
  // Signed-out views get their OWN dirs — see `signedOut`. Tracked so an abort
  // cannot leave them behind.
  const scratchDirs: string[] = []
  // One `ZDOTDIR` per run, cleaned up with the data dirs below.
  const zdotdir = makeShellRcDir()
  scratchDirs.push(zdotdir)

  let token = await mintSessionToken(baseUrl)
  if (!(await sessionValid(baseUrl, token))) {
    throw new Error(
      `the freshly minted session for ${DEMO_EMAIL} does not validate at ${baseUrl} — the instance is not the one that was seeded`
    )
  }
  // Minted on first use, and only if an onboarding view is in scope: most runs
  // never touch the wizard, and a lane that does not need the newcomer should
  // not fail because the seed predates it.
  let newcomerToken: string | undefined

  try {
   for (const view of views) {
    const desktop = view.desktop as DesktopCapture
    const kind = desktop.drive.kind
    // A signed-out launch must not reuse the lane's data dir: the app persists
    // its injected account there, so it would come up SIGNED IN and photograph
    // the shell under a login filename. The wizard needs its own for the same
    // reason PLUS one more — the run's dir already completed onboarding.
    const signedOut = kind === `login`
    const onboarding = kind === `onboarding`

    // Re-check before every launch: a reseed between two views would otherwise
    // photograph the login pane for the rest of the lane (see sessionValid).
    if (!signedOut && !(await sessionValid(baseUrl, token))) {
      console.warn(`[desktop] session went stale (reseed?) — re-minting`)
      token = await mintSessionToken(baseUrl)
      if (!(await sessionValid(baseUrl, token))) {
        throw new Error(
          `session for ${DEMO_EMAIL} will not validate at ${baseUrl}; every remaining view would photograph the sign-in pane`
        )
      }
    }

    const resolved = resolveDesktopVars(desktop, options.ids)
    if (`skip` in resolved) {
      shots.push({ viewId: view.id, state: `skipped`, reason: resolved.skip })
      console.log(`[desktop] ${view.id}: skipped — ${resolved.skip}`)
      continue
    }

    if (onboarding && newcomerToken === undefined) {
      try {
        newcomerToken = await mintSessionToken(baseUrl, NEWCOMER_EMAIL, NEWCOMER_PASSWORD)
      } catch (error) {
        const reason = `no ${NEWCOMER_EMAIL} session: ${error instanceof Error ? error.message : String(error)}`
        shots.push({ viewId: view.id, state: `skipped`, reason })
        console.log(`[desktop] ${view.id}: skipped — ${reason}`)
        continue
      }
    }

    try {
      const file = await captureOne({
        viewId: view.id,
        binary,
        processName,
        baseUrl,
        token: onboarding ? newcomerToken! : token,
        teamId: options.ids.teamId,
        dataDir:
          signedOut || onboarding
            ? scratchDirs[scratchDirs.push(makeDataDir(options.reposRoot)) - 1]!
            : dataDir,
        // The view's own `env` layers on top: a drive says WHERE the app opens,
        // these say what else is already open when it gets there.
        driveVars: resolved.env,
        zdotdir,
        shellCwd: options.reposRoot,
        anchorDelayMs: desktop.anchorDelayMs ?? DEFAULT_ANCHOR_DELAY_MS,
        signedOut,
        onboarding,
      })
      shots.push({ viewId: view.id, state: `captured`, file })
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      shots.push({ viewId: view.id, state: `failed`, reason })
      console.error(`[desktop] ${view.id}: FAILED — ${reason}`)
    }
   }
  } finally {
    // A mint failure or a stale session throws straight past the loop; without
    // this the run leaves an `exp-shots-desktop-*` dir (with a persisted
    // account in it) behind in tmp every time.
    for (const dir of [dataDir, ...scratchDirs]) {
      rmSync(dir, { recursive: true, force: true })
    }
    await pruneStrayDevices()
  }
  return { shots, failures: shots.filter((shot) => shot.state === `failed`).length }
}

/**
 * A throwaway app data dir. `reposRoot` is written as a one-key settings.json:
 * the app merges it over its defaults on load (camelCase, every field
 * optional), so this moves the repos root without touching anything else.
 */
function makeDataDir(reposRoot?: string): string {
  const dir = mkdtempSync(join(tmpdir(), `exp-shots-desktop-`))
  if (reposRoot) {
    writeFileSync(
      join(dir, `settings.json`),
      `${JSON.stringify({ reposRoot }, null, 2)}\n`
    )
  }
  return dir
}

/**
 * A throwaway `ZDOTDIR` holding one `.zshrc` that pins [`SHELL_PROMPT`].
 *
 * zsh reads `$ZDOTDIR/.zshrc` instead of `~/.zshrc`, which is the only hook
 * that reaches an interactive login shell's prompt from outside — `PS1` in the
 * environment is overwritten by whatever rc file runs next. Nothing else is
 * configured on purpose: the point is a shell that looks the same on every
 * machine. macOS-only, like the whole desktop lane; a non-zsh `$SHELL` simply
 * ignores it and keeps its own prompt.
 */
function makeShellRcDir(): string {
  const dir = mkdtempSync(join(tmpdir(), `exp-shots-zdotdir-`))
  writeFileSync(
    join(dir, `.zshrc`),
    [
      `# Written by packages/shots — the committed screenshot store must not`,
      `# carry a developer's account name, hostname or prompt theme.`,
      `PROMPT=${JSON.stringify(SHELL_PROMPT)}`,
      `RPROMPT=''`,
      ``,
    ].join(`\n`)
  )
  return dir
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
  /** Throwaway `ZDOTDIR` pinning the dev shell's prompt — [`makeShellRcDir`]. */
  zdotdir: string
  /**
   * Where `EXP_DEV_OPEN_SHELL`'s shell starts. The tab is titled after this
   * directory's name, so `$HOME` (the default) would name the developer.
   */
  shellCwd?: string
  anchorDelayMs: number
  /**
   * Launch with NO injected session, on a throwaway data dir — the only way to
   * reach the pre-login surfaces, because the shell renders the login pane for
   * any session phase that is not `Synced` and `dev_inject_session` skips
   * everything when `EXP_DEV_SERVER`/`EXP_DEV_TOKEN` are absent.
   */
  signedOut?: boolean
  /**
   * The first-run wizard: signed in as the newcomer, but WITHOUT
   * `EXP_SKIP_ONBOARDING` (which would render the shell instead) and without
   * `EXP_DEV_TEAM` (the newcomer is in no team, and a team id it cannot see
   * would be silently dropped anyway).
   */
  onboarding?: boolean
}

async function captureOne(options: CaptureOneOptions): Promise<string> {
  const out = rawShotPath(`desktop`, options.viewId)
  mkdirSync(dirname(out), { recursive: true })

  for (let attempt = 0; attempt < 2; attempt++) {
    // A FRESH path per attempt: a retry that found the previous attempt's file
    // would "succeed" instantly against a window that has not synced yet, which
    // is the exact failure the handshake exists to remove.
    const readyFile = join(
      mkdtempSync(join(tmpdir(), `exp-shots-ready-`)),
      `${options.viewId}.json`
    )
    rmSync(readyFile, { force: true })
    const child = track(
      Bun.spawn({
        cmd: [options.binary],
        env: {
          ...process.env,
          EXP_INSTANCE_URL: options.baseUrl,
          ...(options.signedOut
            ? {}
            : {
                EXP_DEV_SERVER: options.baseUrl,
                EXP_DEV_TOKEN: options.token,
                // The wizard's whole precondition is "this account has no
                // team"; naming one would be a contradiction, not a hint.
                ...(options.onboarding ? {} : { EXP_DEV_TEAM: options.teamId }),
              }),
          EXP_DATA_DIR: options.dataDir,
          ...(options.onboarding ? {} : { EXP_SKIP_ONBOARDING: `1` }),
          EXP_DEV_READY_FILE: readyFile,
          EXP_WINDOW_SIZE: WINDOW_SIZE,
          HOSTNAME: DEVICE_HOSTNAME,
          ZDOTDIR: options.zdotdir,
          ...(options.shellCwd ? { EXP_DEV_SHELL_CWD: options.shellCwd } : {}),
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
      await waitForReady(readyFile, child, options)
      // Raise BEFORE the settle, not only before the shutter: gpui only
      // repaints a window that is not key on its own schedule, and the
      // deferred layers (an open popover, a dialog's prefilled input) landed
      // reliably only after activation — measured at ~2s after the raise,
      // never within a 300ms grace. The second raise below is the guard
      // against something else having stolen the front in the meantime.
      await raiseWindow(child.pid)
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
      rmSync(dirname(readyFile), { recursive: true, force: true })
    }
  }
  throw new Error(`unreachable`)
}

/**
 * Block until the app writes its ready file (EXP-633).
 *
 * The window appearing proves only that gpui got a surface; everything the shot
 * is ABOUT — the board's rows, the members list, the dialog the drive asked for
 * — arrives over Electric afterwards. The old lane slept six seconds and hoped,
 * which was both wasteful on a warm stack and wrong on a cold one: a skeleton
 * screen is not flat, so the variance gate waved it straight through and the
 * store gained a confidently-empty picture under the right filename.
 *
 * So the app tells us instead. `install_dev_ready_probe` polls its own state
 * (session Synced, every shape Live, any requested dialog settled — or, for the
 * signed-out lane, the auth-config fetch finished) and writes the file once.
 * Until then it logs what it is blocked on every five seconds, which is why the
 * failure message says to run it by hand rather than guessing.
 *
 * Aborts early when the child has exited: a crash on boot would otherwise cost
 * the full timeout, twice, per view.
 */
async function waitForReady(
  readyFile: string,
  child: Child,
  options: CaptureOneOptions
): Promise<void> {
  const deadline = Date.now() + READY_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (existsSync(readyFile)) return
    if (child.exitCode !== null) {
      throw new Error(
        `the app exited (${child.exitCode}) before signalling ready — run it by hand with the same env to see why: ${describe(options.driveVars)}`
      )
    }
    await sleep(READY_POLL_MS)
  }
  throw new Error(
    `never signalled ready within ${READY_TIMEOUT_MS / 1000}s (${describe(options.driveVars)}) — ` +
      `Electric is probably not syncing (a stopped electric container, an unseeded instance, or a ` +
      `dev server serving shapes without their control headers). Run it by hand with the same env ` +
      `plus EXP_DEV_READY_FILE; it logs what it is waiting for every 5s.`
  )
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
      reposRoot: flag(`repos-root`) ?? process.env.SHOTS_REPOS_ROOT,
      dryRun: argv.includes(`--dry-run`),
    })
    // A dry run's whole output IS the plan, and the capture path logs as it
    // goes, so print only what the caller has not already seen.
    if (argv.includes(`--dry-run`)) {
      for (const shot of result.shots) {
        console.log(`[desktop] ${shot.viewId}: ${shot.reason ?? shot.state}`)
      }
    }
    console.log(
      `[desktop] ${result.shots.filter((shot) => shot.state === `captured`).length} captured, ` +
        `${result.shots.filter((shot) => shot.state === `skipped`).length} skipped, ` +
        `${result.failures} failed`
    )
    if (result.failures > 0) process.exit(1)
  }
}
