/**
 * The web recipe registry (EXP-566) — the interactions that turn a route into a
 * view.
 *
 * Most catalog views are a URL and an anchor. The rest need a click first: a
 * popover, a dialog, a tab, an expanded diff. Each of those is one named recipe
 * here, and `views.json` refers to it BY NAME — the view-catalog test parses
 * this file and fails if the two lists disagree in either direction, so a recipe
 * can never be renamed out from under the manifest.
 *
 * Two rules every recipe follows:
 *
 *   1. It waits for what it clicks. The capturer runs the recipe BEFORE the
 *      view's anchor, because most anchors describe text the recipe reveals
 *      ("Create an account", "Priority", "@Composable"). A recipe that clicks
 *      blind races Electric's first paint.
 *   2. It ends on an unambiguous post-state. The manifest's anchor is a
 *      human-readable label and is often satisfied by the TRIGGER as well as by
 *      the opened surface ("New issue", "Start coding", "New automation"), so
 *      the recipe does the strict wait and the anchor is the readable summary.
 *
 * Recipes never sign in. Views on `/auth/*` and `/onboarding` run in a fresh
 * anonymous context — see `needsAuth` in `capture-views.ts`.
 */
import type { Locator, Page } from "@playwright/test"
import {
  DEMO_DEVICE_LABEL,
  DEMO_EMAIL,
  DEMO_FEED_QUESTION,
  DEMO_NAME,
  DEMO_PASSWORD,
  TEAM_SLUG,
} from "../screenshot-demo"

/** The seeded board every board-scoped view is captured on. */
const BOARD_SLUG = `mobile-app`

/** Everything a recipe is allowed to know about the instance it is driving. */
export interface RecipeCtx {
  baseUrl: string
  demo: {
    email: string
    password: string
    name: string
    teamSlug: string
    boardSlug: string
    deviceLabel: string
    feedQuestion: string
  }
}

export type Recipe = (page: Page, ctx: RecipeCtx) => Promise<void>

export function recipeContext(baseUrl: string): RecipeCtx {
  return {
    baseUrl,
    demo: {
      email: DEMO_EMAIL,
      password: DEMO_PASSWORD,
      name: DEMO_NAME,
      teamSlug: TEAM_SLUG,
      boardSlug: BOARD_SLUG,
      deviceLabel: DEMO_DEVICE_LABEL,
      feedQuestion: DEMO_FEED_QUESTION,
    },
  }
}

/** `waitFor` as a boolean — recipes branch on presence more than they assert it. */
async function appears(locator: Locator, timeout: number): Promise<boolean> {
  return locator
    .first()
    .waitFor({ timeout })
    .then(() => true)
    .catch(() => false)
}

/**
 * The phone layout hides the sidebar behind an off-canvas sheet, so a
 * sidebar-footer trigger is not even mounted until it is opened.
 */
async function openSidebar(page: Page): Promise<void> {
  const toggle = page.getByRole(`button`, { name: `Toggle Sidebar` })
  if (await appears(toggle, 2_000)) await toggle.first().click()
}

// ------------------------------------------------------------------- auth

/**
 * Flip the merged sign-in page into sign-up mode. Runs UNAUTHENTICATED: an
 * existing session would bounce `/auth/login` to the team. The toggle only
 * renders when password auth AND public sign-up are both on (routes/auth/
 * login.tsx), which is dev's default.
 */
async function recipeOpenRegister(page: Page): Promise<void> {
  const toggle = page.getByRole(`button`, { name: `Create one`, exact: true })
  if (!(await appears(toggle, 20_000))) {
    throw new Error(
      `no "Create one" toggle on /auth/login — public sign-up is off ` +
        `(AUTH_SIGNUP_ENABLED) or password auth is disabled`
    )
  }
  await toggle.click()
  await page.getByText(`Create an account`).first().waitFor({ timeout: 10_000 })
}

// ----------------------------------------------------------------- issues

/**
 * Open the board's filter popover on its category list (Status · Priority ·
 * Labels). The trigger's accessible name grows a count badge once filters are
 * active, hence the prefix match.
 */
async function recipeOpenFilterPopover(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: /^Filter/ })
  await trigger.first().waitFor({ timeout: 20_000 })
  await trigger.first().click()
  await page.getByText(`Priority`, { exact: true }).first().waitFor({ timeout: 10_000 })
}

/**
 * Bring the issue's comment thread into frame. The composer is the last thing
 * in the timeline, so scrolling it into view puts the whole conversation on
 * screen; on the phone layout the composer lives in the floating bottom bar
 * instead, so the Activity header is the fallback target.
 */
async function recipeScrollToComments(page: Page): Promise<void> {
  const composer = page.getByPlaceholder(`Leave a reply…`)
  const target = (await appears(composer, 10_000))
    ? composer.first()
    : page.getByText(/^Activity/).first()
  await target.scrollIntoViewIfNeeded({ timeout: 15_000 })
  // Let the smooth-scroll land before the anchor wait starts measuring.
  await page.waitForTimeout(400)
}

/**
 * Open the create-issue editor. The only always-present trigger on a NON-empty
 * board is the sidebar's icon link (desktop) / tab-bar FAB (phone); the visible
 * "New issue" button in the board body belongs to the empty state.
 */
async function recipeOpenCreateIssue(page: Page): Promise<void> {
  const trigger = page
    .getByRole(`link`, { name: `New issue` })
    .or(page.getByRole(`button`, { name: `New issue` }))
  await trigger.first().waitFor({ timeout: 20_000 })
  await trigger.first().click()
  await page.getByTestId(`issue-editor-create`).waitFor({ timeout: 15_000 })
}

/**
 * Open the cross-board issue search. The keyboard route (Cmd/Ctrl+F, the
 * Linear-style global shortcut in routes/t/$teamSlug/route.tsx) works at both
 * viewports; the icon trigger is the fallback because the sidebar carries it on
 * desktop and the tab bar on the phone.
 */
async function recipeOpenSearch(page: Page): Promise<void> {
  const input = page.getByPlaceholder(`Search issues...`)
  await page.keyboard.press(`ControlOrMeta+f`)
  if (await appears(input, 4_000)) return
  const button = page.getByRole(`button`, { name: `Search`, exact: true })
  await button.first().waitFor({ timeout: 15_000 })
  await button.first().click()
  await input.first().waitFor({ timeout: 15_000 })
}

// ----------------------------------------------------------------- coding

/**
 * Open the launch dialog from an issue. The capsule renders only for a member
 * on a repo-backed board WITH a device online, so this is the view that needs
 * `bun run screenshots:desktop` alongside the seed — with no device the page
 * shows the plain "No desktop online" line instead, and there is nothing to
 * click.
 */
async function recipeOpenStartCoding(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `Start coding`, exact: true })
  if (!(await appears(trigger, 30_000))) {
    throw new Error(
      `no "Start coding" control on the issue — it needs STEER_RELAY_URL, a ` +
        `repo-backed board and an ONLINE device: run bun run screenshots:desktop`
    )
  }
  await trigger.first().click()
  // "Start coding" is also the dialog's title and its submit button, so anchor
  // on something only the opened launcher has.
  await page.getByText(`Plan mode`).first().waitFor({ timeout: 15_000 })
}

/**
 * Expand the issue's live coding session and wait for the scripted transcript's
 * final unanswered question. An empty feed still renders the feed container, so
 * waiting on the container alone happily photographs a "Reconnecting…" state
 * when the relay is unreachable.
 */
async function recipeOpenAgentDock(page: Page, ctx: RecipeCtx): Promise<void> {
  // Desktop-web renders a labelled Watch button; the phone layout collapses it
  // into the 52px session FAB (EXP-568, aria-label "Open coding session").
  const watch = page.getByRole(`button`, { name: `Watch` })
  const fab = page.getByRole(`button`, { name: `Open coding session` })
  if (await appears(watch, 15_000)) {
    await watch.first().click()
  } else if (await appears(fab, 15_000)) {
    await fab.first().click()
  } else {
    throw new Error(
      `no Watch button and no session FAB — is the relay stub running and the ` +
        `session yours? (EXP-312 keeps live sessions owner-only)`
    )
  }
  await page
    .getByText(ctx.demo.feedQuestion.slice(0, 40))
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 30_000 })
}

// ---------------------------------------------------------------- reviews

/**
 * Expand the biggest file in the review diff so the shot shows an actual patch
 * rather than a file list. The diff is fetched live from GitHub — the slowest
 * and flakiest view in the catalog, hence the generous timeout.
 */
async function recipeExpandFirstDiffFile(page: Page): Promise<void> {
  const file = page.getByText(`TopicScreen.kt`)
  await file.first().waitFor({ timeout: 60_000 })
  await file.first().click()
  await page.getByText(`@Composable`).first().waitFor({ timeout: 30_000 })
}

// ---------------------------------------------------------------- support

/**
 * Open the freshest helpdesk conversation so the right pane isn't the empty
 * state. Anchors on the reporter's closing line, which only the opened thread
 * carries — the list row shows the thread title and a snippet.
 */
async function recipeOpenFirstThread(page: Page): Promise<void> {
  // Visible-filtered on both ends: the mobile layout keeps the thread list in
  // the DOM under the pushed conversation, so an unfiltered `.first()` can pick
  // a hidden copy of the text and wait on it forever.
  const row = page.getByText(`Emma Fischer`).filter({ visible: true })
  await row.first().waitFor({ timeout: 30_000 })
  await row.first().click()
  await page
    .getByText(`thank you for the quick turnaround`)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 30_000 })
}

// ---------------------------------------------------------------- actions

/** Open the editor for the seeded "Update dependencies" action (owner-only). */
async function recipeOpenActionEditor(page: Page): Promise<void> {
  const menu = page.getByRole(`button`, { name: `Action menu for Update dependencies` })
  if (!(await appears(menu, 20_000))) {
    throw new Error(
      `no action menu for "Update dependencies" — the seed is stale, or the ` +
        `session is not a team owner (editing is owner-only)`
    )
  }
  await menu.first().click()
  await page.getByRole(`menuitem`, { name: `Edit`, exact: true }).click()
  await page.getByRole(`heading`, { name: `Edit action` }).waitFor({ timeout: 15_000 })
}

/** Switch to the Automations tab and open the create-automation editor. */
async function recipeOpenAutomationEditor(page: Page): Promise<void> {
  const tab = page.getByRole(`tab`, { name: `Automations` })
  await tab.first().waitFor({ timeout: 20_000 })
  await tab.first().click()
  const create = page.getByRole(`button`, { name: `New automation` })
  if (!(await appears(create, 15_000))) {
    throw new Error(
      `no "New automation" button — creating needs STEER_RELAY_URL and an owner session`
    )
  }
  await create.first().click()
  // The trigger and the dialog heading share the label; the submit button does not.
  await page.getByRole(`button`, { name: `Create automation` }).waitFor({ timeout: 15_000 })
}

// -------------------------------------------------------- getting started

/**
 * Open the sidebar-footer "Getting started" sheet (EXP-88). EXP-548 hides the
 * entry entirely once every checklist item is done — and while the signals are
 * still loading — so an absent entry is a seed problem, not a flake, and says so.
 */
async function recipeOpenGettingStarted(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `Getting started`, exact: true })
  if (!(await appears(trigger, 15_000))) {
    await openSidebar(page)
  }
  if (!(await appears(trigger, 10_000))) {
    throw new Error(
      `no "Getting started" sidebar entry — EXP-548 hides it once every ` +
        `checklist entry is complete. Re-seed so at least one stays incomplete: ` +
        `bun run seed:screenshots`
    )
  }
  await trigger.first().click()
  await page.getByText(`Set up the coding loop`).first().waitFor({ timeout: 15_000 })
}

/**
 * The registry itself. Keys are the names `views.json` uses; the view-catalog
 * test parses this literal, so keep it a flat name-to-function table with the
 * closing brace in column 0.
 */
export const RECIPES: Record<string, Recipe> = {
  openRegister: recipeOpenRegister,
  openFilterPopover: recipeOpenFilterPopover,
  scrollToComments: recipeScrollToComments,
  openCreateIssue: recipeOpenCreateIssue,
  openSearch: recipeOpenSearch,
  openStartCoding: recipeOpenStartCoding,
  openAgentDock: recipeOpenAgentDock,
  expandFirstDiffFile: recipeExpandFirstDiffFile,
  openFirstThread: recipeOpenFirstThread,
  openActionEditor: recipeOpenActionEditor,
  openAutomationEditor: recipeOpenAutomationEditor,
  openGettingStarted: recipeOpenGettingStarted,
}
