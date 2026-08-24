/**
 * The browser engine every web capture shares (EXP-566).
 *
 * Lifted verbatim out of `capture-social-shots.ts`, which grew it first: the
 * dark-mode context, the "What's new" pre-dismissal, the login walk, the
 * settle-after-anchor beat and the anchor waiter. Both the manifest-driven
 * `capture-views.ts` and the social-card capture now sit on top of it, so the
 * two lanes cannot drift into photographing subtly different apps.
 *
 * Nothing here knows what a "view" is — that is the catalog's job. This file
 * only knows how to drive one Chromium page at a fixed viewport.
 */
import type { Browser, BrowserContext, Page } from "@playwright/test"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { latestChangelogEntry } from "@/lib/changelog"
import { DEMO_EMAIL, DEMO_PASSWORD } from "../screenshot-demo"

/** The geometry half of a form factor. */
export interface ContextOptions {
  viewport: { width: number; height: number }
  deviceScaleFactor: number
  /** Phone form factors: mobile UA + touch, so the app renders its phone layout. */
  isMobile?: boolean
}

/**
 * A context pinned to the demo instance's conditions: self-signed certs
 * accepted (captures go through the Caddy h2 proxy on https://localhost:3000),
 * dark theme — the app forces `html.dark` anyway, but the browser chrome and
 * form controls follow this — and a fixed locale/timezone so relative dates and
 * number formats are byte-stable between runs.
 */
export async function launchContext(
  browser: Browser,
  options: ContextOptions
): Promise<BrowserContext> {
  const context = await browser.newContext({
    viewport: options.viewport,
    deviceScaleFactor: options.deviceScaleFactor,
    isMobile: options.isMobile,
    hasTouch: options.isMobile,
    ignoreHTTPSErrors: true,
    colorScheme: `dark`,
    locale: `en-US`,
    timezoneId: `Europe/Berlin`,
  })
  // Pre-dismiss the sidebar "What's new" card — a screenshot store shouldn't
  // carry a release-specific banner that changes every ship (key + semantics:
  // lib/changelog-seen.ts).
  const head = latestChangelogEntry()
  if (head) {
    await context.addInitScript(
      ([id]) => window.localStorage.setItem(`exp.changelogSeenId`, id),
      [head.id]
    )
  }
  return context
}

/** Sign the demo user in and wait until a team route has taken over. */
export async function login(
  page: Page,
  baseUrl: string,
  credentials?: { email: string; password: string },
  /**
   * Where a successful sign-in lands. The demo user has a team, so it is
   * `/t/...`; the team-less newcomer (EXP-566) is bounced to `/onboarding`
   * instead, and waiting for a team URL would time out on the happy path.
   */
  landing: string = `**/t/**`
): Promise<void> {
  await page.goto(`${baseUrl}/auth/login`)
  await page.fill(`#email`, credentials?.email ?? DEMO_EMAIL)
  await page.fill(`#password`, credentials?.password ?? DEMO_PASSWORD)
  await page.getByRole(`button`, { name: `Sign in`, exact: true }).click()
  await page.waitForURL(landing, { timeout: 30_000 })
}

/**
 * Give Electric a beat after the anchor text lands: avatars, icon fonts and the
 * remaining shapes stream in just behind the first paint. `networkidle` is
 * best-effort — the app holds 19 long-polls open, so it usually never fires and
 * the 10s cap is the real timer.
 */
export async function settle(page: Page, extraMs = 1_200): Promise<void> {
  await page.waitForLoadState(`networkidle`, { timeout: 10_000 }).catch(() => {})
  await page.waitForTimeout(extraMs)
}

/** What the capturer waits for before it presses the shutter (catalog `Anchor`). */
export interface AnchorSpec {
  text?: string
  testId?: string
  timeoutMs?: number
}

/**
 * Wait for a view's anchor. `testId` wins when both are given — it is the
 * stabler of the two. An anchor with neither is a manifest bug, so it throws
 * rather than silently photographing whatever happens to be on screen.
 */
export async function waitForAnchor(page: Page, anchor: AnchorSpec): Promise<void> {
  const timeout = anchor.timeoutMs ?? 30_000
  // Filter to VISIBLE matches: the mobile layout keeps the list route in the
  // DOM under a pushed detail view, so `.first()` alone can land on a hidden
  // copy of the text and wait on it forever.
  if (anchor.testId) {
    await page.getByTestId(anchor.testId).filter({ visible: true }).first().waitFor({ timeout })
    return
  }
  if (anchor.text) {
    await page.getByText(anchor.text).filter({ visible: true }).first().waitFor({ timeout })
    return
  }
  throw new Error(`anchor has neither text nor testId`)
}

/** Write the PNG, creating the directory tree on the way. */
export async function shot(
  page: Page,
  outPath: string,
  options: { fullPage?: boolean } = {}
): Promise<void> {
  mkdirSync(dirname(outPath), { recursive: true })
  await page.screenshot({ path: outPath, fullPage: options.fullPage ?? false })
}
