/**
 * Capture every web view in the catalog (EXP-566).
 *
 * The manifest — `packages/view-catalog/views.json` — is the shot list: this
 * script owns no list of its own. It resolves each view's route placeholders
 * against the seeded demo instance, drives the browser to it (optionally via a
 * recipe from `lib/view-recipes.ts`), waits for the view's anchor and writes a
 * raw PNG. Downscaling to the store's nominal frame is a later stage's job.
 *
 * Prereqs (same stack as the store screenshots, see seed-screenshots.ts):
 *   1. backend up + the BUILT web server on :5173 behind the Caddy h2 proxy on
 *      :3000 — Electric's 19 long-polls starve Chromium's 6-connection HTTP/1.1
 *      limit, so captures go through https://localhost:3000
 *   2. bun run seed:screenshots
 *   3. bun run screenshots:desktop   # the `steering` view needs a live relay
 *
 * Then, from apps/web:
 *   bun run capture:views
 *   bun run capture:views -- --form-factor web --views board,issue-detail
 *
 * Writes <out>/<form-factor>/<view-id>.png, defaulting to <repo-root>/.shots-raw
 * (NOT committed — EXP-348 keeps screenshots out of git).
 */
import { chromium, type BrowserContext, type Page } from "@playwright/test"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { statSync } from "node:fs"
import {
  captureFor,
  viewsFor,
  type Platform,
  type WebCapture,
  type WebIdentity,
} from "@exp/view-catalog"
import { launchContext, login, settle, shot, waitForAnchor } from "./lib/capture-web"
import { RECIPES, recipeContext, type RecipeCtx } from "./lib/view-recipes"
import {
  DEMO_EMAIL,
  DEMO_INVITE_TOKEN,
  DEMO_PASSWORD,
  NEWCOMER_EMAIL,
  NEWCOMER_PASSWORD,
} from "./screenshot-demo"

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, `../../..`)

/** The two browser form factors the catalog knows about. */
type FormFactor = Extract<Platform, `web` | `web-mobile`>

const FORM_FACTORS: Record<
  FormFactor,
  { viewport: { width: number; height: number }; deviceScaleFactor: number; isMobile?: boolean }
> = {
  web: { viewport: { width: 1440, height: 960 }, deviceScaleFactor: 2 },
  [`web-mobile`]: {
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 3,
    isMobile: true,
  },
}

/** Both form factors, in the order a run walks them. */
const FORM_FACTORS_ORDER: readonly FormFactor[] = [`web`, `web-mobile`]

/**
 * Who a view is photographed as, when the manifest does not say.
 *
 * `/auth/*` redirects a signed-in user straight to their team, so the sign-in
 * and sign-up shots have to be anonymous. Everything else is the demo user —
 * including `/onboarding` and `/invite/$token`, which are NOT anonymous views:
 * both need a real session that simply has no team yet, so the manifest marks
 * them `auth: "newcomer"` explicitly.
 */
function identityFor(capture: WebCapture): WebIdentity {
  if (capture.auth) return capture.auth
  return capture.route.startsWith(`/auth`) ? `anonymous` : `demo`
}

const CREDENTIALS: Record<Exclude<WebIdentity, `anonymous`>, Credentials> = {
  demo: { email: DEMO_EMAIL, password: DEMO_PASSWORD, landing: `**/t/**` },
  // Signing in with no team lands on the wizard, never on a team route.
  newcomer: {
    email: NEWCOMER_EMAIL,
    password: NEWCOMER_PASSWORD,
    landing: `**/onboarding**`,
  },
}

interface Credentials {
  email: string
  password: string
  landing: string
}

/**
 * The one route placeholder that needs the DATABASE, not a constant: the
 * reporter's magic link is an HMAC over a thread id that only exists after a
 * seed. Kept out of `resolveRoute` so the browser lane never imports the db
 * layer unless a view it actually wants asks for it.
 */
const DB_PLACEHOLDER = `$supportToken`

/** Placeholder substitution against the seeded demo instance. */
function resolveRoute(route: string, ctx: RecipeCtx, supportToken?: string): string {
  return route
    .replaceAll(`$teamSlug`, ctx.demo.teamSlug)
    .replaceAll(`$boardSlug`, ctx.demo.boardSlug)
    .replaceAll(`$inviteToken`, DEMO_INVITE_TOKEN)
    .replaceAll(DB_PLACEHOLDER, supportToken ?? ``)
}

/**
 * Mint the reporter magic link, once, and only when something wants it.
 *
 * DYNAMIC import on purpose: `lib/demo-ids.ts` pulls in `@/db/connection`, and a
 * `--views board` run has no business opening a database connection (or failing
 * because `DATABASE_URL` is not exported on the capture host). Returns
 * `undefined` rather than throwing — the token is a credential the host may
 * legitimately not be able to mint (no `BETTER_AUTH_SECRET`), and one skipped
 * view is a better outcome than a failed lane.
 */
async function resolveSupportToken(): Promise<string | undefined> {
  try {
    const { resolveDemoIds } = await import(`./lib/demo-ids`)
    const ids = await resolveDemoIds()
    return ids.supportToken
  } catch (err) {
    console.warn(
      `  could not resolve ${DB_PLACEHOLDER}: ${err instanceof Error ? err.message : String(err)}`
    )
    return undefined
  }
}

interface Args {
  formFactors: FormFactor[]
  viewIds: string[] | null
  baseUrl: string
  out: string
}

function parseArgs(argv: string[]): Args {
  const flags = new Map<string, string>()
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]!
    if (!arg.startsWith(`--`)) continue
    const eq = arg.indexOf(`=`)
    if (eq !== -1) {
      flags.set(arg.slice(2, eq), arg.slice(eq + 1))
    } else {
      flags.set(arg.slice(2), argv[i + 1] ?? ``)
      i += 1
    }
  }

  const factor = flags.get(`form-factor`) ?? `all`
  const formFactors: FormFactor[] =
    factor === `all`
      ? [`web`, `web-mobile`]
      : factor === `web` || factor === `web-mobile`
        ? [factor]
        : (() => {
            throw new Error(`--form-factor must be web, web-mobile or all (got "${factor}")`)
          })()

  const views = flags.get(`views`)?.trim()
  return {
    formFactors,
    viewIds: views ? views.split(`,`).map((id) => id.trim()).filter(Boolean) : null,
    baseUrl: (flags.get(`base-url`) || process.env.CAPTURE_BASE_URL || `https://localhost:3000`).replace(
      /\/$/,
      ``
    ),
    out: resolve(flags.get(`out`) || resolve(REPO_ROOT, `.shots-raw`)),
  }
}

interface Result {
  formFactor: FormFactor
  viewId: string
  /** PNG size in bytes, or the first line of the error that stopped the shot. */
  bytes?: number
  error?: string
}

function fileSize(path: string): number | undefined {
  try {
    return statSync(path).size
  } catch {
    return undefined
  }
}

function printSummary(results: Result[]): void {
  const width = Math.max(...results.map((r) => `${r.formFactor}/${r.viewId}`.length), 12)
  console.log(`\n${`view`.padEnd(width)}  result`)
  console.log(`${`-`.repeat(width)}  ------`)
  for (const result of results) {
    const label = `${result.formFactor}/${result.viewId}`.padEnd(width)
    const detail =
      result.error !== undefined
        ? `FAIL  ${result.error}`
        : `ok    ${((result.bytes ?? 0) / 1024).toFixed(0)} KB`
    console.log(`${label}  ${detail}`)
  }
}

/** Drive one view and press the shutter. Throws; the caller records it. */
async function captureView(
  page: Page,
  capture: WebCapture,
  ctx: RecipeCtx,
  outPath: string,
  supportToken?: string
): Promise<void> {
  await page.goto(`${ctx.baseUrl}${resolveRoute(capture.route, ctx, supportToken)}`)

  // Recipe FIRST, anchor after: most recipe-driven views anchor on text the
  // recipe itself reveals ("Create an account", "Priority", "@Composable"), and
  // every recipe already waits for the trigger it clicks. So the anchor always
  // describes the FINAL photographed state, which is what the manifest means.
  const recipe = capture.recipe ? RECIPES[capture.recipe] : undefined
  if (capture.recipe && !recipe) {
    throw new Error(`unknown recipe "${capture.recipe}" — not in lib/view-recipes.ts`)
  }
  if (recipe) await recipe(page, ctx)

  await waitForAnchor(page, capture.anchor)
  await settle(page, capture.settleMs)
  await shot(page, outPath, { fullPage: capture.fullPage })
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  const ctx = recipeContext(args.baseUrl)

  console.log(`base ${args.baseUrl}`)
  console.log(`out  ${args.out}`)

  // One lookup for the whole run, and only when a wanted view needs it.
  const wantsToken = FORM_FACTORS_ORDER.some((formFactor) =>
    args.formFactors.includes(formFactor)
      ? viewsFor(formFactor).some(
          (view) =>
            (!args.viewIds || args.viewIds.includes(view.id)) &&
            (captureFor(view, formFactor) as WebCapture).route.includes(DB_PLACEHOLDER)
        )
      : false
  )
  const supportToken = wantsToken ? await resolveSupportToken() : undefined

  const browser = await chromium.launch()
  const results: Result[] = []

  try {
    for (const formFactor of args.formFactors) {
      const wanted = viewsFor(formFactor).filter(
        (view) => !args.viewIds || args.viewIds.includes(view.id)
      )
      if (args.viewIds) {
        const unknown = args.viewIds.filter((id) => !wanted.some((view) => view.id === id))
        for (const id of unknown) {
          console.warn(`  skip ${id} — no ${formFactor} capture in the catalog`)
        }
      }
      if (wanted.length === 0) continue

      console.log(`\n=== ${formFactor} (${wanted.length} views) ===`)
      const geometry = FORM_FACTORS[formFactor]

      // One signed-in context per IDENTITY per form factor; the anonymous views
      // each get a throwaway one so no session cookie can leak into a /auth shot.
      const sessions = new Map<
        Exclude<WebIdentity, `anonymous`>,
        { context: BrowserContext; page: Page }
      >()

      try {
        for (const view of wanted) {
          const capture = captureFor(view, formFactor) as WebCapture
          const outPath = resolve(args.out, formFactor, `${view.id}.png`)
          const identity = identityFor(capture)

          if (capture.route.includes(DB_PLACEHOLDER) && !supportToken) {
            // Skipped, not failed: without the token the route 404s and the shot
            // would be a "link expired" card filed under the view's name.
            console.warn(
              `  skip  ${view.id} — no ${DB_PLACEHOLDER} (re-seed, and export ` +
                `BETTER_AUTH_SECRET so it can be minted)`
            )
            continue
          }

          let page: Page
          let throwaway: BrowserContext | null = null
          if (identity === `anonymous`) {
            throwaway = await launchContext(browser, geometry)
            page = await throwaway.newPage()
          } else {
            // One long-lived context per identity: signing in is the slowest
            // step in the lane, and the two never share cookies.
            let session = sessions.get(identity)
            if (!session) {
              const credentials = CREDENTIALS[identity]
              const context = await launchContext(browser, geometry)
              const signedIn = await context.newPage()
              console.log(`  signing in as ${credentials.email}`)
              await login(signedIn, args.baseUrl, credentials, credentials.landing)
              session = { context, page: signedIn }
              sessions.set(identity, session)
            }
            page = session.page
          }

          try {
            await captureView(page, capture, ctx, outPath, supportToken)
            results.push({ formFactor, viewId: view.id, bytes: fileSize(outPath) })
            console.log(`  ok    ${view.id}`)
          } catch (err) {
            const message = err instanceof Error ? err.message.split(`\n`)[0]! : String(err)
            results.push({ formFactor, viewId: view.id, error: message })
            console.error(`  FAIL  ${view.id} — ${message}`)
            // Best-effort evidence of what was actually on screen.
            await shot(page, resolve(args.out, formFactor, `_failed-${view.id}.png`)).catch(
              () => {}
            )
          } finally {
            if (throwaway) await throwaway.close()
          }
        }
      } finally {
        for (const session of sessions.values()) await session.context.close()
      }
    }
  } finally {
    await browser.close()
  }

  if (results.length === 0) {
    console.log(`\nnothing to capture`)
    return
  }
  printSummary(results)
  const failed = results.filter((result) => result.error !== undefined)
  console.log(`\n${results.length - failed.length} captured, ${failed.length} failed → ${args.out}`)
  if (failed.length > 0) process.exit(1)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
