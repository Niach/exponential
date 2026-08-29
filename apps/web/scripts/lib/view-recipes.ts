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

/**
 * What `openSearch` types, and the seeded issue title it must bring back.
 * Both halves are pinned here because `views.json` anchors the SEARCH view on
 * the result row — the query and the anchor have to keep matching the same
 * seeded issue (`seed-screenshots.ts`).
 */
const SEARCH_QUERY = `cold start`
const SEARCH_RESULT_TITLE = `Reduce cold start below 800 ms`

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

// ------------------------------------------------------------- onboarding

/**
 * Step the first-run wizard from its choice card onto the name-your-team form.
 * Runs as the NEWCOMER (a session with no team) — the demo user has one, so the
 * route resolves a default team and the wizard opens on the board step instead,
 * where there is no choice card to click.
 *
 * The choice BUTTON and the create step's CardTitle both read "Create a team"
 * (wizard.tsx choice step / create step), so the post-state wait is the form's
 * own name field. Stops there deliberately: submitting would mint a real team
 * and move the wizard on to the board step.
 */
async function recipeOpenOnboardingCreateTeam(page: Page): Promise<void> {
  // The button's accessible name carries its caption too ("Create a team Start
  // fresh. You'll be the owner."), hence the prefix match.
  const trigger = page.getByRole(`button`, { name: /^Create a team/ })
  if (!(await appears(trigger, 20_000))) {
    throw new Error(
      `no "Create a team" choice on /onboarding — the session already has a ` +
        `team, so the wizard skipped to the board step. Capture this view as ` +
        `the team-less newcomer (auth: "newcomer")`
    )
  }
  await trigger.first().click()
  await page.locator(`#onb-team-name`).waitFor({ timeout: 15_000 })
}

/**
 * The wizard's other branch: the paste-an-invite-link form. Same newcomer-only
 * precondition, and the same title collision — "Join a team" labels both the
 * choice button and the step it opens — so this waits on the link field.
 * Continuing would navigate to `/invite/$token` and burn the invite.
 */
async function recipeOpenOnboardingJoin(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: /^Join a team/ })
  if (!(await appears(trigger, 20_000))) {
    throw new Error(
      `no "Join a team" choice on /onboarding — the session already has a ` +
        `team, so the wizard skipped to the board step. Capture this view as ` +
        `the team-less newcomer (auth: "newcomer")`
    )
  }
  await trigger.first().click()
  await page.locator(`#onb-invite-link`).waitFor({ timeout: 15_000 })
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
 * screen; the phone layout keeps its composer in a floating bottom bar
 * instead, so there the Activity header is the target.
 *
 * Which of the two applies is decided by the VIEWPORT, not by racing the
 * composer against a timeout (EXP-669). It used to be the race, and the race
 * was losable: on a cold-synced run the reply box needed longer than its 10s
 * to mount, the recipe quietly took the phone branch, and
 * `scrollIntoViewIfNeeded` on an Activity header that was already on screen
 * did nothing at all — so the shot came back framed on the issue header, a
 * different picture under the same filename, roughly one run in three. A
 * missing composer at md+ is now a failure rather than a silent second frame.
 */
async function recipeScrollToComments(page: Page): Promise<void> {
  const width = page.viewportSize()?.width ?? 0
  const hasComposer = width >= 768
  const target = hasComposer
    ? page.getByPlaceholder(`Leave a reply…`).first()
    : page.getByText(/^Activity/).first()
  // Deliberately the same 20s the other recipes give a control they know is
  // mounted: long enough for a first Electric sync, and loud when it is not.
  await target.waitFor({ timeout: 20_000 })
  await target.scrollIntoViewIfNeeded({ timeout: 15_000 })
  // …then align it to the BOTTOM of the frame (EXP-670).
  // `scrollIntoViewIfNeeded` moves the MINIMUM distance that makes the target
  // visible and does nothing at all when it already is, so where it comes to
  // rest depends on where the page happened to be — and this view carries a
  // deliberately tight 0.001 diffTolerance, so a ~15px difference in resting
  // offset rewrote all three of its shots with nothing behind it.
  // Scrolling the CONTAINER to its end is absolute rather than relative: the
  // composer is the last thing in the timeline, so its scroller's bottom is
  // both the frame this view wants (whole thread, composer and its toolbar)
  // and a fixed point every run lands on identically. Which ancestor actually
  // scrolls is not knowable from the styles alone — the walk tries each
  // candidate and only accepts one whose `scrollTop` actually moved.
  //
  // ONLY on the composer path. The phone layout keeps its composer in a
  // floating bottom bar, so its target is the Activity header, and "scroll its
  // container to the end" would mean something quite different there — it
  // reframes the shot past the header it was asked to reach.
  if (hasComposer) {
    await target.evaluate((node) => {
      for (let el = node.parentElement; el; el = el.parentElement) {
        if (el.scrollHeight <= el.clientHeight + 1) continue
        const before = el.scrollTop
        el.scrollTop = el.scrollHeight
        if (el.scrollTop !== before) return
      }
      window.scrollTo(0, document.documentElement.scrollHeight)
    })
  }
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
  if (!(await appears(input, 4_000))) {
    const button = page.getByRole(`button`, { name: `Search`, exact: true })
    await button.first().waitFor({ timeout: 15_000 })
    await button.first().click()
    await input.first().waitFor({ timeout: 15_000 })
  }
  // An empty search box photographs the placeholder and a "start typing" hint,
  // which says nothing about what search DOES. Type a seeded query and wait for
  // the row it matches — the anchor in views.json is that same issue title.
  await input.first().fill(SEARCH_QUERY)
  await page
    .getByText(SEARCH_RESULT_TITLE)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
}

/**
 * Open the phone issue's properties sheet — status, priority, assignee, labels,
 * due date and board, all from the bottom bar's pull-up.
 *
 * Mobile-only by construction: at md+ the same properties are inline in the
 * issue's right-hand rail and the bar carrying this trigger is not mounted at
 * all, so a wide viewport reads as a manifest mistake and fails loudly rather
 * than waiting 20s for a button that can never exist.
 */
async function recipeOpenIssuePropertiesMobile(page: Page): Promise<void> {
  const width = page.viewportSize()?.width ?? 0
  if (width >= 768) {
    throw new Error(
      `the properties sheet is web-mobile only — the issue bottom bar carrying ` +
        `it is md:hidden, and this viewport is ${width}px wide`
    )
  }
  const trigger = page.getByRole(`button`, { name: `Issue properties`, exact: true })
  await trigger.first().waitFor({ timeout: 20_000 })
  await trigger.first().click()
  await page.getByTestId(`issue-properties-sheet`).waitFor({ timeout: 15_000 })
}

/**
 * Put the board into a multi-selection so the bulk action bar renders. The two
 * layouts enter selection differently (issue-list.tsx): at md+ every row has a
 * hover-revealed checkbox, below md that column is `display: none` and the
 * entry point is the row's context menu — Radix owns the touch long-press — and
 * from then on a plain tap toggles.
 *
 * Property edits keep the selection alive, so the recipe stops at the bar: it
 * never opens one of its menus, and it certainly never confirms the delete.
 */
async function recipeOpenBoardBulkEdit(page: Page): Promise<void> {
  const rows = page.locator(`[data-testid^="issue-row-"]`)
  await rows.first().waitFor({ timeout: 20_000 })
  const wanted = Math.min(await rows.count(), 3)
  if (wanted < 2) {
    throw new Error(
      `the board has ${wanted} issue row(s) — a bulk selection needs at least ` +
        `two. Re-seed: bun run seed:screenshots`
    )
  }
  // The checkbox column is `opacity-0` until hover, which Playwright still
  // counts as visible — so its presence is a clean layout probe.
  const boxes = page.getByRole(`checkbox`, { name: /^Select / })
  if (await appears(boxes, 5_000)) {
    for (let index = 0; index < wanted; index += 1) {
      await boxes.nth(index).click()
    }
  } else {
    await rows.first().click({ button: `right` })
    await page.getByRole(`menuitem`, { name: `Select`, exact: true }).click()
    for (let index = 1; index < wanted; index += 1) {
      await rows.nth(index).click()
    }
  }
  await page
    .getByTestId(`bulk-action-bar`)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
}

/**
 * Open the phone board switcher from the mobile topbar's board name. The
 * topbar itself is `md:hidden`, so at the desktop viewport there is no trigger
 * at all — that reads as a manifest mistake rather than a flake, so it fails
 * loudly instead of waiting 20s for a button that can never mount.
 */
async function recipeOpenBoardSwitcher(page: Page): Promise<void> {
  const width = page.viewportSize()?.width ?? 0
  if (width >= 768) {
    throw new Error(
      `the board switcher is web-mobile only — the mobile topbar carrying it ` +
        `is md:hidden, and this viewport is ${width}px wide`
    )
  }
  const trigger = page.getByRole(`button`, { name: `Switch board`, exact: true })
  if (!(await appears(trigger, 20_000))) {
    throw new Error(
      `no "Switch board" control in the mobile topbar — the topbar swaps the ` +
        `board name for a section title on inbox/devices/actions/reviews/` +
        `support/settings, so capture this view on a board route`
    )
  }
  await trigger.first().click()
  await page.getByRole(`dialog`, { name: `Boards` }).waitFor({ timeout: 15_000 })
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
 * The launcher is ONE dialog over three tabs (EXP-257/EXP-615) — Issues picks
 * work off the board, Actions runs a saved definition, Chat is a free prompt on
 * a repo's default branch. Each is its own view, so each gets its own shot.
 *
 * The per-tab device candidates differ (Chat needs a chat-capable device,
 * Actions an actions-capable one), which is exactly why the empty-tab case is
 * worth photographing too: the tab still renders, it just has nothing to run on.
 */
async function openLaunchTab(page: Page, name: string, settled: Locator): Promise<void> {
  await recipeOpenStartCoding(page)
  const tab = page.getByRole(`tab`, { name, exact: true })
  await tab.first().waitFor({ timeout: 10_000 })
  await tab.first().click()
  await settled.first().waitFor({ timeout: 15_000 })
}

/** The launcher's Actions tab: the saved-action picker and its typed inputs. */
async function recipeOpenStartCodingActions(page: Page): Promise<void> {
  await openLaunchTab(page, `Actions`, page.getByPlaceholder(`Search actions…`))
}

/** The launcher's Chat tab: a free prompt on a repository's default branch. */
async function recipeOpenStartCodingChat(page: Page): Promise<void> {
  await openLaunchTab(page, `Chat`, page.getByText(`Repository`, { exact: true }))
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

// --------------------------------------------------------------- machines

/**
 * Open a machine's Device settings dialog from the Devices page. The ⋯ menu
 * only renders for a REGISTERED device of the caller's own (my-machines.tsx),
 * so with no relay stub running there is no row and nothing to click.
 *
 * The menu button's accessible name carries the device label, hence the prefix
 * match — it keeps the recipe working when the seed renames the machine.
 */
async function recipeOpenMachineSettings(page: Page): Promise<void> {
  const menu = page.getByRole(`button`, { name: /^Machine menu for/ })
  if (!(await appears(menu, 30_000))) {
    throw new Error(
      `no machine ⋯ menu under "My machines" — the row needs a REGISTERED ` +
        `device of your own: run bun run screenshots:desktop (and set ` +
        `STEER_RELAY_URL)`
    )
  }
  await menu.first().click()
  await page.getByRole(`menuitem`, { name: `Edit`, exact: true }).click()
  await page.getByRole(`heading`, { name: `Device settings` }).waitFor({ timeout: 15_000 })
}

/**
 * Open the "Add a server" dialog — the CLI install one-liner. The section
 * header's trailing button is always mounted (no device or relay needed), so
 * the wait is on the snippet itself: the dialog's own heading appears a frame
 * before the `pre` is laid out, and the snippet IS the view.
 */
async function recipeOpenAddServer(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `Add server`, exact: true })
  await trigger.first().waitFor({ timeout: 20_000 })
  await trigger.first().click()
  await page.getByText(/EXP_INSTANCE=/).first().waitFor({ timeout: 15_000 })
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

/**
 * Open the create-action dialog — the describe-it-and-the-agent-writes-it
 * creator (EXP-257). The trigger rides the Actions section header only for an
 * OWNER on a steer-enabled instance, so its absence is a precondition failure.
 * "New action" is also the dialog's own title, so the post-state wait is the
 * form's name field.
 */
async function recipeOpenActionCreate(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `New action`, exact: true })
  if (!(await appears(trigger, 20_000))) {
    throw new Error(
      `no "New action" button — creating needs STEER_RELAY_URL and a team-OWNER ` +
        `session (authoring is owner-only)`
    )
  }
  await trigger.first().click()
  await page.locator(`#create-action-name`).waitFor({ timeout: 15_000 })
}

/**
 * The automations list plus its "Recent automated runs" section
 * (automations-tab.tsx), one step short of `openAutomationEditor`. EXP-686
 * gave automations their own desktop route, so the tab strip exists on the
 * PHONE only — click it when it is there, and otherwise the page already IS
 * the view. The runs header is the anchor because it renders unconditionally:
 * the list above it can legitimately be the "No automations yet" empty state,
 * and waiting on a row would photograph a half-mounted view whenever the
 * automations shape has not landed yet.
 */
async function clickTabIfPresent(page: Page, name: string): Promise<void> {
  const tab = page.getByRole(`tab`, { name, exact: true })
  if (!(await appears(tab, 5_000))) return
  await tab.first().click()
}

async function recipeOpenAutomationsTab(page: Page): Promise<void> {
  await clickTabIfPresent(page, `Automations`)
  await page
    .getByText(`Recent automated runs`)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
}

/**
 * The shipped seed catalog that prefills the creator run. EXP-686 split its
 * two homes: the phone keeps it as the Actions page's third tab, while a
 * desktop viewport reaches it through the icon-only "Suggestions" lightbulb in
 * the Actions header, which opens Getting started on its Suggested actions
 * tab. Click whichever of the two is mounted. The seeds are constants
 * (`lib/action-suggestions.ts`), never DB rows, so the anchor can be a
 * specific seed title rather than a header the trigger shares.
 */
async function recipeOpenSuggestionsTab(page: Page): Promise<void> {
  const tab = page.getByRole(`tab`, { name: `Suggestions`, exact: true })
  if (await appears(tab, 5_000)) {
    await tab.first().click()
  } else {
    const lightbulb = page.getByRole(`button`, {
      name: `Suggestions`,
      exact: true,
    })
    if (!(await appears(lightbulb, 20_000))) {
      throw new Error(
        `neither a Suggestions tab nor the Suggestions lightbulb — the seeds ` +
          `live on the Actions page (phone) or in Getting started (desktop)`
      )
    }
    await lightbulb.first().click()
  }
  await page
    .getByText(`Daily standup digest`)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
}

/**
 * Open the create-automation editor. On the phone that means switching to the
 * Actions page's Automations tab first; a desktop viewport is already on
 * `/automations` and has no tab strip (EXP-686).
 */
async function recipeOpenAutomationEditor(page: Page): Promise<void> {
  await clickTabIfPresent(page, `Automations`)
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

// --------------------------------------------------------------- settings

/**
 * Open the widget editor on its Appearance tab, where the live whole-panel
 * preview renders from the real `@exp/widget/widget.css` (EXP-435). Two waits,
 * because two things can be missing: the dialog (owner-only surface — "New
 * widget" is not mounted for a member) and the tab's preview, which is the
 * point of the view. Ends without saving; the form is untouched.
 */
async function recipeOpenWidgetEditor(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `New widget`, exact: true })
  if (!(await appears(trigger, 20_000))) {
    throw new Error(
      `no "New widget" button on settings/widget — the widget surface is ` +
        `owner-only (canManageWidgets)`
    )
  }
  await trigger.first().click()
  // "New widget" labels the dialog too, so settle on the form's name field.
  await page.locator(`#widget-name`).waitFor({ timeout: 15_000 })
  await page.getByRole(`tab`, { name: `Appearance`, exact: true }).click()
  await page
    .getByText(`Widget preview`, { exact: true })
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
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
  openOnboardingCreateTeam: recipeOpenOnboardingCreateTeam,
  openOnboardingJoin: recipeOpenOnboardingJoin,
  openFilterPopover: recipeOpenFilterPopover,
  scrollToComments: recipeScrollToComments,
  openCreateIssue: recipeOpenCreateIssue,
  openSearch: recipeOpenSearch,
  openBoardBulkEdit: recipeOpenBoardBulkEdit,
  openBoardSwitcher: recipeOpenBoardSwitcher,
  openIssuePropertiesMobile: recipeOpenIssuePropertiesMobile,
  openStartCoding: recipeOpenStartCoding,
  openStartCodingActions: recipeOpenStartCodingActions,
  openStartCodingChat: recipeOpenStartCodingChat,
  openAgentDock: recipeOpenAgentDock,
  expandFirstDiffFile: recipeExpandFirstDiffFile,
  openFirstThread: recipeOpenFirstThread,
  openMachineSettings: recipeOpenMachineSettings,
  openAddServer: recipeOpenAddServer,
  openActionEditor: recipeOpenActionEditor,
  openActionCreate: recipeOpenActionCreate,
  openAutomationsTab: recipeOpenAutomationsTab,
  openSuggestionsTab: recipeOpenSuggestionsTab,
  openAutomationEditor: recipeOpenAutomationEditor,
  openWidgetEditor: recipeOpenWidgetEditor,
  openGettingStarted: recipeOpenGettingStarted,
}
