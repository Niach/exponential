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
 *
 * EXP-566: the browser engine and the three interactions this needs now live in
 * `lib/capture-web.ts` and `lib/view-recipes.ts`, shared with the catalog-driven
 * `capture-views.ts`. The eight shots below are still this script's own list —
 * social cards are a curated subset with their own output names, not the
 * catalog's — but they are photographed by exactly the same machinery.
 */
import { chromium, type Page } from "@playwright/test"
import { mkdirSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { DEMO_EMAIL, TEAM_SLUG } from "./screenshot-demo"
import { launchContext, login, settle, shot } from "./lib/capture-web"
import { RECIPES, recipeContext } from "./lib/view-recipes"

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT = resolve(HERE, `../../marketing/social-shots`)
const BASE = process.env.CAPTURE_BASE_URL ?? `https://localhost:3000`

const T = `/t/${TEAM_SLUG}`

interface SocialShot {
  /** Output file name, without the extension. These are load-bearing: the card
   *  generator in apps/marketing reads them by name. */
  name: string
  /** Omitted = stay on the page the previous shot left behind. */
  route?: string
  /** Visible text to wait for before the shot (and before the recipe). */
  anchor: string
  anchorTimeoutMs?: number
  /** A key from lib/view-recipes.ts, run after `anchor` lands. */
  recipe?: string
}

const SHOTS: SocialShot[] = [
  // The busy seeded Mobile App board.
  { name: `board`, route: `${T}/boards/mobile-app`, anchor: `Ship onboarding flow v2` },
  // Markdown showcase + comments + the live Coding-now row.
  { name: `issue`, route: `${T}/boards/mobile-app/issues/APP-5`, anchor: `Coding now` },
  // Steering — expand the agent dock on the issue's own running session. No
  // route: it deliberately continues from the `issue` shot's page.
  { name: `steering`, anchor: `Coding now`, recipe: `openAgentDock` },
  // The cross-board open-PR queue.
  { name: `reviews`, route: `${T}/reviews`, anchor: `Batch-edit labels from the board` },
  // APP-14 renders a REAL diff fetched from GitHub; expand the biggest file so
  // the shot shows an actual patch, not just the file list.
  {
    name: `review-diff`,
    route: `${T}/reviews/APP-14`,
    anchor: `TopicScreen.kt`,
    anchorTimeoutMs: 60_000,
    recipe: `expandFirstDiffFile`,
  },
  { name: `actions`, route: `${T}/agents`, anchor: `Update dependencies` },
  // Inbox — 3 unread.
  { name: `inbox`, route: `${T}/inbox`, anchor: `Mira Chen assigned you APP-6` },
  // The helpdesk thread list shows reporter + snippet; open the freshest
  // conversation so the right pane isn't the empty state.
  { name: `support`, route: `${T}/support`, anchor: `Emma Fischer`, recipe: `openFirstThread` },
]

async function capture(page: Page, spec: SocialShot): Promise<void> {
  if (spec.route) await page.goto(`${BASE}${spec.route}`)
  await page
    .getByText(spec.anchor)
    .first()
    .waitFor({ timeout: spec.anchorTimeoutMs ?? 30_000 })
  if (spec.recipe) {
    const recipe = RECIPES[spec.recipe]
    if (!recipe) throw new Error(`unknown recipe "${spec.recipe}"`)
    await recipe(page, recipeContext(BASE))
  }
  await settle(page)
  await shot(page, resolve(OUT, `${spec.name}.png`))
  console.log(`captured ${spec.name}.png`)
}

async function main() {
  mkdirSync(OUT, { recursive: true })
  const browser = await chromium.launch()
  const context = await launchContext(browser, {
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 2,
  })
  const page = await context.newPage()

  console.log(`logging in as ${DEMO_EMAIL} at ${BASE}`)
  await login(page, BASE)

  for (const spec of SHOTS) await capture(page, spec)

  await browser.close()
  console.log(`done → ${OUT}`)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
