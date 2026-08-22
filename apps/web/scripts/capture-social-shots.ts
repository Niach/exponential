/**
 * Capture the web-app screenshots the social-media cards are built from
 * (EXP-597) — the browser-sized sibling of the mobile store captures.
 *
 * Prereqs (same stack as the store screenshots, see seed-screenshots.ts):
 *   1. backend up + the BUILT web server on :5173 (behind the Caddy h2 proxy
 *      on :3000 — Electric's 19 long-polls starve Chromium's 6-connection
 *      HTTP/1.1 limit, so captures go through https://localhost:3000)
 *   2. bun run seed:screenshots
 *   3. bun run screenshots:desktop   # steering + Coding-now need the relay
 *
 * Then, from apps/web:
 *   bun run screenshots:social
 *
 * Writes raw PNGs (3200×2000, a 1600×1000 viewport at 2x) into
 * apps/marketing/social-shots/ — NOT committed (EXP-348: screenshots stay out
 * of git); the committed cards are composed from them by
 * apps/marketing/scripts/generate-social.tsx.
 */
import { chromium, type Page } from "@playwright/test"
import { mkdirSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { DEMO_EMAIL, DEMO_FEED_QUESTION, DEMO_PASSWORD, TEAM_SLUG } from "./screenshot-demo"
import { latestChangelogEntry } from "@/lib/changelog"

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT = resolve(HERE, `../../marketing/social-shots`)
const BASE = process.env.CAPTURE_BASE_URL ?? `https://localhost:3000`

const T = `/t/${TEAM_SLUG}`

/** Give Electric a beat after the anchor text lands: avatars, icon fonts and
 *  the remaining shapes stream in just behind the first paint. */
async function settle(page: Page) {
  await page.waitForLoadState(`networkidle`, { timeout: 10_000 }).catch(() => {})
  await page.waitForTimeout(1_200)
}

async function shot(page: Page, name: string) {
  await page.screenshot({ path: resolve(OUT, `${name}.png`) })
  console.log(`captured ${name}.png`)
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const browser = await chromium.launch()
  const context = await browser.newContext({
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 2,
    ignoreHTTPSErrors: true,
    colorScheme: `dark`,
  })
  // Pre-dismiss the sidebar "What's new" card — ad shots shouldn't carry a
  // release-specific banner (key + semantics: lib/changelog-seen.ts).
  const head = latestChangelogEntry()
  if (head) {
    await context.addInitScript(
      ([id]) => window.localStorage.setItem(`exp.changelogSeenId`, id),
      [head.id]
    )
  }
  const page = await context.newPage()

  console.log(`logging in as ${DEMO_EMAIL} at ${BASE}`)
  await page.goto(`${BASE}/auth/login`)
  await page.fill(`#email`, DEMO_EMAIL)
  await page.fill(`#password`, DEMO_PASSWORD)
  await page.getByRole(`button`, { name: `Sign in`, exact: true }).click()
  await page.waitForURL(`**/t/**`, { timeout: 30_000 })

  // Board — the busy seeded Mobile App board.
  await page.goto(`${BASE}${T}/boards/mobile-app`)
  await page.getByText(`Ship onboarding flow v2`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `board`)

  // Issue detail — markdown showcase + comments + the live Coding-now row.
  await page.goto(`${BASE}${T}/boards/mobile-app/issues/APP-5`)
  await page.getByText(`Coding now`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `issue`)

  // Steering — expand the agent dock on the issue's own running session and
  // wait for the scripted feed's final unanswered question.
  await page.getByRole(`button`, { name: `Watch` }).first().click()
  await page
    .getByText(DEMO_FEED_QUESTION.slice(0, 40))
    .first()
    .waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `steering`)

  // Reviews — the cross-board open-PR queue.
  await page.goto(`${BASE}${T}/reviews`)
  await page.getByText(`Batch-edit labels from the board`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `reviews`)

  // Review detail — APP-14 renders a REAL diff fetched from GitHub; expand the
  // biggest file so the shot shows an actual patch, not just the file list.
  await page.goto(`${BASE}${T}/reviews/APP-14`)
  await page.getByText(`TopicScreen.kt`).first().waitFor({ timeout: 60_000 })
  await page.getByText(`TopicScreen.kt`).first().click()
  await page.getByText(`@Composable`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `review-diff`)

  // Actions.
  await page.goto(`${BASE}${T}/agents`)
  await page.getByText(`Update dependencies`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `actions`)

  // Inbox — 3 unread.
  await page.goto(`${BASE}${T}/inbox`)
  await page.getByText(`Mira Chen assigned you APP-6`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `inbox`)

  // Support — the helpdesk thread list shows reporter + snippet; open the
  // freshest conversation so the right pane isn't the empty state.
  await page.goto(`${BASE}${T}/support`)
  await page.getByText(`Emma Fischer`).first().waitFor({ timeout: 30_000 })
  await page.getByText(`Emma Fischer`).first().click()
  await page.getByText(`thank you for the quick turnaround`).first().waitFor({ timeout: 30_000 })
  await settle(page)
  await shot(page, `support`)

  await browser.close()
  console.log(`done → ${OUT}`)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
