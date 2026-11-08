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
 *      the opened surface ("New issue", "Device settings", "New automation"), so
 *      the recipe does the strict wait and the anchor is the readable summary.
 *
 * Recipes never sign in. Views on `/auth/*` and `/onboarding` run in a fresh
 * anonymous context — see `needsAuth` in `capture-views.ts`.
 */
import type { Locator, Page } from "@playwright/test"
import { emptyStateCopy } from "@/lib/empty-states"
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
// #NEW-21: a creator query, so the shot shows `by:` narrowing to Alex's issues.
const SEARCH_QUERY = `by:alex`
const SEARCH_RESULT_TITLE = `Reduce cold start below 800 ms`

/**
 * What `typeIssueRef` types into the Agent composer: a prompt, then `#` and a
 * token. `APP` is the seeded board's key, so every issue matches and the list
 * shows its uncapped, scrolling face (the desktop `search-list` query too).
 */
const ISSUE_REF_PROMPT = `Explain #`
const ISSUE_REF_TOKEN = `APP`

/** The seeded Working hours entry on APP-15 (seed-screenshots.ts) whose Post
 *  to the issue raises the consent card. */
const APP_CONSENT_ENTRY = `docs, admin`

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
/**
 * VAPP-122: the phone-width `app` view photographs the PREVIEW tab (the app
 * rendered through the host with its records), like the iOS and Android
 * walks; #EXP-1732: the editor is THE session screen (its `session-tabs`
 * SegmentedControl), a phone opens on Chat.
 */
async function recipeOpenAppPreviewTab(page: Page): Promise<void> {
  const tabs = page.getByTestId(`session-tabs`)
  await tabs.waitFor({ state: `visible`, timeout: 30_000 })
  await tabs.getByRole(`radio`, { name: `Preview` }).or(tabs.getByRole(`tab`, { name: `Preview` })).or(tabs.getByText(`Preview`, { exact: true })).first().click()
  await page.getByTestId(`app-editor-preview`).waitFor({ state: `visible`, timeout: 30_000 })
}

/** VAPP-121: the app editor's Export, Include records switched on.
 *  #EXP-1732: Export lives in the header's `…` menu on every width. */
async function recipeOpenAppExport(page: Page): Promise<void> {
  const menu = page.getByTestId(`app-editor-menu`)
  await menu.waitFor({ state: `visible`, timeout: 30_000 })
  await menu.click()
  const item = page.getByTestId(`app-editor-menu-export`)
  await item.waitFor({ state: `visible`, timeout: 15_000 })
  await item.click()
  const dialog = page.getByTestId(`app-export-dialog`)
  await dialog.waitFor({ state: `visible`, timeout: 15_000 })
  await page.getByTestId(`app-export-records`).click()
}

/** VAPP-121: New app → Import…, the fixture's capture text pasted, the
 *  server's issue rows listed. */
async function recipeOpenAppImport(page: Page): Promise<void> {
  const { default: fixture } = await import(`@exp/domain-contract/fixtures/app-export.json`, { with: { type: `json` } })
  // VAPP-242: Import… is THE NEW MENU's second item (the sidebar's or the
  // phone band's `+`).
  const plus = page.getByTestId(`apps-new`).filter({ visible: true }).first()
  await plus.waitFor({ state: `visible`, timeout: 15_000 })
  await plus.click()
  const importButton = page.getByTestId(`apps-import`).filter({ visible: true }).first()
  await importButton.waitFor({ state: `visible`, timeout: 15_000 })
  await importButton.click()
  await page.getByTestId(`app-import-text`).fill(fixture.capture.importText)
  await page.getByTestId(`app-import-issues`).waitFor({ state: `visible`, timeout: 15_000 })
}

/** VAPP-217: the RecordTable's Add: the RecordForm for the kind. */
async function recipeOpenRecordForm(page: Page): Promise<void> {
  const add = page.getByRole(`button`, { name: `Add` }).filter({ visible: true }).first()
  await add.waitFor({ state: `visible`, timeout: 30_000 })
  await add.click()
  await page.getByRole(`dialog`).getByRole(`button`, { name: `Save` }).waitFor({ state: `visible`, timeout: 15_000 })
}

/** The Data tab's first row `…` menu (the row actions sit after the toolbar). */
async function openDataRowMenu(page: Page): Promise<void> {
  const more = page.getByTestId(`app-data-row-menu`).filter({ visible: true }).first()
  await more.waitFor({ state: `visible`, timeout: 30_000 })
  await more.click()
}

/** VAPP-211: a Data row's Edit: the form with its History rows. */
async function recipeOpenRecordHistory(page: Page): Promise<void> {
  await openDataRowMenu(page)
  await page.getByRole(`menuitem`, { name: `Edit` }).click()
  await page.getByTestId(`app-data-history`).waitFor({ state: `visible`, timeout: 15_000 })
}

/** VAPP-211: delete a row: the Undo toast. The row is added first, so the lanes
 *  (every form factor, one stack) never eat each other's seeded records. */
async function recipeOpenRecordUndo(page: Page): Promise<void> {
  const name = `Undo ${Date.now()}`
  await page.evaluate(async (name) => {
    const shape = (await (await fetch(`/api/shapes/apps?offset=-1`)).json()) as { value?: { id: string; slug: string } }[]
    const id = shape.find((m) => m.value?.slug === `crm`)?.value?.id
    await fetch(`/api/trpc/apps.records.upsert`, { method: `POST`, headers: { "content-type": `application/json` }, body: JSON.stringify({ id, kind: `company`, data: { name, stage: `lead` }, via: `editor` }) })
  }, name)
  const row = page.getByTestId(`app-data-row`).filter({ hasText: name }).filter({ visible: true }).first()
  await row.waitFor({ state: `visible`, timeout: 30_000 })
  await row.getByTestId(`app-data-row-menu`).click()
  await page.getByRole(`menuitem`, { name: `Delete` }).click()
  const confirm = page.getByRole(`alertdialog`).or(page.getByRole(`dialog`)).getByRole(`button`, { name: /^Delete/ })
  if (await appears(confirm, 1_500)) await confirm.click()
  await page.getByRole(`button`, { name: `Undo` }).waitFor({ state: `visible`, timeout: 15_000 })
}

/** VAPP-219: the Data tab's Import…, a CSV pasted, the mapping + preview shown. */
async function recipeOpenCsvImport(page: Page): Promise<void> {
  const button = page.getByRole(`button`, { name: /^Import/ }).filter({ visible: true }).first()
  await button.waitFor({ state: `visible`, timeout: 30_000 })
  await button.click()
  const dialog = page.getByRole(`dialog`)
  await dialog.locator(`textarea`).fill(`name,stage,domain\nInitech,lead,initech.example\nUmbrella,active,umbrella.example\nHooli,nosuch,hooli.example\n`)
  await dialog.getByRole(`button`, { name: `Import 2` }).waitFor({ state: `visible`, timeout: 15_000 })
}

/** eApps V1: the last Versions row's Preview (an old version, read-only). */
async function recipeOpenAppVersionPreview(page: Page): Promise<void> {
  const preview = page.getByRole(`button`, { name: `Preview` }).filter({ visible: true }).last()
  await preview.waitFor({ state: `visible`, timeout: 30_000 })
  await preview.click()
  await page.getByTestId(`app-editor-viewing`).filter({ visible: true }).first().waitFor({ state: `visible`, timeout: 15_000 })
}

/** VAPP-243: Settings down to its last group: the MCP tools, Automations and Webhooks. */
async function recipeScrollAppSettingsEnd(page: Page): Promise<void> {
  const row = page.getByTestId(`app-settings-webhooks`).filter({ visible: true }).first()
  await row.waitFor({ state: `visible`, timeout: 30_000 })
  await row.scrollIntoViewIfNeeded()
}

/**
 * EXP-1558: THE consent card (VAPP-120, app-actions.json) the way a person
 * raises it, as the natives' specimen words it: the seeded Working hours
 * app (nothing granted, `grantedTools: []`), its 8 h "docs, admin" entry
 * on APP-15 opened from the month list, then the entry dialog's Post to the
 * issue, which asks for `exponential.comments.create`. Never answers it.
 * The entry is in the seed's October: step the month back until it shows.
 */
async function recipeOpenAppConsent(page: Page): Promise<void> {
  if (!(await appears(page.getByTestId(`app-editor-preview`).filter({ visible: true }), 15_000))) {
    await recipeOpenAppPreviewTab(page)
  }
  const preview = page.getByTestId(`app-editor-preview`).filter({ visible: true }).first()
  const row = preview.getByText(APP_CONSENT_ENTRY, { exact: true }).filter({ visible: true }).first()
  for (let step = 0; step < 3 && !(await appears(row, 8_000)); step += 1) {
    // The month header's first outline icon button is `prev` (the template's
    // toolbar: prev · month · next).
    await preview.getByRole(`button`).filter({ hasNotText: /\S/ }).first().click()
  }
  await row.waitFor({ timeout: 10_000 })
  await row.click()
  const post = page.getByRole(`button`, { name: `Post to the issue`, exact: true })
  await post.waitFor({ timeout: 15_000 })
  await post.click()
  await page.getByTestId(`exponential-ui-consent`).waitFor({ timeout: 15_000 })
  // EXP-1676: the card alone over the editor, as desktop and the natives shoot
  // it: the entry dialog that raised it stays open but out of the picture.
  await page.addStyleTag({ content: `.xui-Dialog-content { visibility: hidden !important; }` })
}

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

/**
 * EXP-725: the wizard's invite step. Runs as the STARTER (owns a board-less
 * team): the route's `?step=invite` search param is the capture hook that
 * starts a resumed wizard past the board step, so nothing is ever created.
 * Waits for the generate button — that proves `teams.inviteCapacity` answered
 * and the control is visible, which is the state the shot documents.
 */
async function recipeOpenOnboardingInvite(page: Page, ctx: RecipeCtx): Promise<void> {
  await page.goto(`${ctx.baseUrl}/onboarding?step=invite`)
  const generate = page.getByTestId(`invite-generate`)
  if (!(await appears(generate, 20_000))) {
    throw new Error(
      `no invite step on /onboarding?step=invite — capture this view as the ` +
        `STARTER (auth: "starter"), the seeded owner of a board-less team`
    )
  }
}

/**
 * The last wizard step, same identity and hook (`?step=devices`). The install
 * one-liner is the view, so the wait is on its EXP_INSTANCE line.
 */
async function recipeOpenOnboardingDevices(page: Page, ctx: RecipeCtx): Promise<void> {
  await page.goto(`${ctx.baseUrl}/onboarding?step=devices`)
  const snippet = page.getByText(/EXP_INSTANCE=/)
  if (!(await appears(snippet, 20_000))) {
    throw new Error(
      `no devices step on /onboarding?step=devices — capture this view as the ` +
        `STARTER (auth: "starter"), the seeded owner of a board-less team`
    )
  }
}

// ----------------------------------------------------------------- issues

/**
 * Bring the issue's comment chat into frame. The composer is the last thing
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
  // The LAST placeholder is the chat's bottom composer; the chated reply
  // boxes above it carry the same placeholder since EXP-1245 and `.first()`
  // landed the frame on the first comment instead (2026-10-10).
  const target = hasComposer
    ? page.getByPlaceholder(`Leave a reply…`).last()
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
  // both the frame this view wants (whole chat, composer and its toolbar)
  // and a fixed point every run lands on identically. Which ancestor actually
  // scrolls is not knowable from the styles alone — the walk tries each
  // candidate and only accepts one whose `scrollTop` actually moved.
  //
  // The phone path too (EXP-1259): its Activity header already peeks out
  // under the floating bottom bar on the first frame, so
  // `scrollIntoViewIfNeeded` counted it visible and never moved, and the shot
  // came back identical to `issue-detail`. The chat is the end of the
  // phone's scroller as well, so its end is the frame there too (the natives'
  // `sg_issue-comments` lands on the last comments the same way).
  // Twice: the chat below the fold can still be laying out after the first
  // scroll (EXP-1245 renders the activity feed progressively), which left the
  // first pass short of the composer.
  for (let pass = 0; pass < 2; pass += 1) {
    await target.evaluate((node) => {
      for (let el = node.parentElement; el; el = el.parentElement) {
        if (el.scrollHeight <= el.clientHeight + 1) continue
        const before = el.scrollTop
        el.scrollTop = el.scrollHeight
        if (el.scrollTop !== before) return
      }
      window.scrollTo(0, document.documentElement.scrollHeight)
    })
    await page.waitForTimeout(300)
  }
  // Let the smooth-scroll land before the anchor wait starts measuring.
  await page.waitForTimeout(400)
}

/**
 * Open the cross-board issue search. The keyboard route (Cmd/Ctrl+F, the
 * Linear-style global shortcut in routes/t/$teamSlug/route.tsx) works at both
 * viewports; the icon trigger is the fallback because the sidebar carries it on
 * desktop and the tab bar on the phone.
 */
async function recipeOpenSearch(page: Page): Promise<void> {
  const input = page.getByPlaceholder(`Search issues`, { exact: true })
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
    // #EXP-1388: a touch long-press selects at once; a menu's Select only
    // where one still opens.
    await rows.filter({ visible: true }).first().click({ button: `right` })
    const item = page.getByRole(`menuitem`, { name: `Select`, exact: true })
    if (await appears(item, 2_000)) await item.click()
    // Tap each row's leading selection check: the row's middle holds the
    // status glyph, whose own picker would open instead.
    const checks = page.locator(`[data-testid^="issue-select-"]`).filter({ visible: true })
    await checks.first().waitFor({ timeout: 10_000 })
    for (let index = 1; index < wanted; index += 1) {
      await checks.nth(index).click()
    }
  }
  await page
    .getByTestId(`bulk-action-bar`)
    .filter({ visible: true })
    .first()
    .waitFor({ timeout: 15_000 })
}

/**
 * #EXP-1388: the Chats list in selection mode, entered the way a person
 * does, like `openBoardBulkEdit`: wide = the rows' hover-revealed checkbox
 * column (two clicks); a phone = the row's long-press (the menu host turns a
 * touch long-press into the selection; a fine pointer gets the menu's
 * Select), then a tap on the next row.
 */
async function recipeOpenChatSelection(page: Page): Promise<void> {
  const rows = page.locator(`[data-testid="chat-row"]`)
  await rows.first().waitFor({ timeout: 20_000 })
  const bar = page.getByTestId(`bulk-action-bar`).filter({ visible: true }).first()
  // Wide: the rows' checkbox column; phones: the row's long-press (a touch
  // menu open = the selection), then taps.
  const select = async (indices: number[]) => {
    const boxes = page.getByRole(`checkbox`, { name: /^Select / })
    if (await boxes.first().isVisible().catch(() => false)) {
      for (const index of indices) await boxes.nth(index).click()
      return
    }
    const [first, ...rest] = indices
    await rows.nth(first!).click({ button: `right` })
    const item = page.getByRole(`menuitem`, { name: `Select`, exact: true })
    if (await appears(item, 2_000)) await item.click()
    for (const index of rest) await rows.nth(index).click()
  }
  // The shot shows the Archived entry (#EXP-1388): archive the last chat
  // once when the seed has none archived yet.
  if (!(await appears(page.getByTestId(`chat-archived`), 1_500))) {
    await select([(await rows.count()) - 1])
    await bar.getByRole(`button`, { name: `Archive`, exact: true }).click()
    await page.getByTestId(`chat-archived`).waitFor({ timeout: 15_000 })
    await bar.waitFor({ state: `hidden`, timeout: 15_000 }).catch(() => {})
  }
  const wanted = Math.min(await rows.count(), 2)
  if (wanted < 2) {
    throw new Error(
      `the Chats list has ${wanted} row(s) — a selection needs at least two. Re-seed: bun run seed:screenshots`
    )
  }
  await select([0, 1])
  await bar.waitFor({ timeout: 15_000 })
  // Off the rows: the shot shows the selection, not a hovered row.
  await page.mouse.move(0, 0)
}

// ---------------------------------------------------------------- reviews

/**
 * Expand the biggest file in the review diff so the shot shows an actual patch
 * rather than a file list. The diff is fetched live from GitHub — the slowest
 * and flakiest view in the catalog, hence the generous timeout.
 *
 * EXP-895: the file COLUMN is the reliable target (`diff-nav-row-<path>`, one
 * per file, and a click there expands the card AND scrolls to it). The card's
 * own header is the fallback for a single-file diff, which draws no column.
 */
/**
 * Expand the FIRST file of the review's diff (EXP-895: the ONE diff view; the
 * review page renders every file card collapsed, `defaultCollapsed`). The md+
 * file column's rows jump-open a file, but the phone has no column (its list is
 * a sheet off the work bar), so the recipe opens the first card from its OWN
 * header, which both form factors draw. The card is a Radix collapsible: its
 * trigger carries `data-state`, so an already-open card is left alone.
 *
 * The strict post-state is a rendered `@@ … @@` hunk row inside that card; the
 * manifest's anchor is a line of the seeded PR's first-file patch.
 */
async function recipeExpandFirstDiffFile(page: Page): Promise<void> {
  const card = page.locator(`[data-testid="file-diff-card"]`).first()
  const trigger = card.locator(`button[data-state]`).first()
  // The diff is fetched live from GitHub, the slowest load in the catalog.
  await trigger.waitFor({ timeout: 60_000 })
  // EXP-916: cards start OPEN everywhere, so this is normally a no-op — the
  // click is only left for a file past the contract's collapse threshold.
  if ((await trigger.getAttribute(`data-state`)) !== `open`) {
    await trigger.click()
  }
  await card
    .locator(`[data-diff-row="hunk"]`)
    .first()
    .waitFor({ timeout: 30_000 })
}

// ------------------------------------------------------------------ agent

/** The Agent composer's field, focused (the page autofocuses it on a pointer,
 *  a click covers the phone). */
async function focusComposer(page: Page): Promise<Locator> {
  // EXP-1558: first let the agents shape land. The agent row re-picks its
  // default (the Assistant) when it arrives and the composer starts afresh,
  // which wiped a draft typed into a cold page (search-list came back as an
  // empty composer). Soft: a team without agents only has Temporary session.
  await page
    .locator(`[data-testid="composer-agent-row"] [data-testid^="composer-agent-"]:not([data-testid="composer-agent-label"])`)
    .nth(1)
    .waitFor({ timeout: 15_000 })
    .catch(() => {})
  const field = page.getByTestId(`agent-composer-field`)
  await field.first().waitFor({ timeout: 30_000 })
  await field.first().click()
  return field.first()
}

/**
 * SLOP-28: THE search list as the `#` typeahead (search-list.json). Types a
 * prompt, then `#`: the list opens at the caret (a bottom sheet on phones)
 * with ITS input focused, so the token typed next lands in that input, as a
 * person typing straight through would. Ends once the input holds the token
 * and the rows are back.
 */
async function recipeTypeIssueRef(page: Page): Promise<void> {
  await focusComposer(page)
  await page.keyboard.type(ISSUE_REF_PROMPT, { delay: 40 })
  const input = page.getByTestId(`search-list-input`).filter({ visible: true })
  await input.first().waitFor({ timeout: 15_000 })
  await page.keyboard.type(ISSUE_REF_TOKEN, { delay: 60 })
  await page
    .getByTestId(`search-list-row`)
    .filter({ visible: true })
    .nth(1)
    .waitFor({ timeout: 15_000 })
  const typed = await input.first().inputValue()
  if (typed !== ISSUE_REF_TOKEN) {
    throw new Error(
      `the # search input holds "${typed}", not "${ISSUE_REF_TOKEN}": the ` +
        `token went somewhere else (did the list take focus?)`
    )
  }
}

/**
 * EXP-1249: the composer's ONE "+" menu open at its root (composer-menu.json
 * test ids), Temporary session picked so every row shows.
 */
async function recipeOpenComposerMenuTemporary(page: Page): Promise<void> {
  // EXP-1551: the four clients photograph ONE state, the phones' sheet: the
  // agent row on Temporary session (composer-home.json `agentRow`) and the +
  // menu open at its ROOT with every runner and work row (composer-menu.json
  // case 1), no submenu open. The row renders once the agents shape is in,
  // after the composer.
  const temporary = page
    .getByTestId(`composer-agent-temporary`)
    .filter({ visible: true })
    .first()
  await temporary.waitFor({ timeout: 20_000 })
  await temporary.click()
  const plus = page.getByTestId(`agent-composer-plus-button`)
  await plus.first().waitFor({ timeout: 30_000 })
  await plus.first().click()
  await page.getByTestId(`agent-composer-menu-device`).first().waitFor({ timeout: 15_000 })
  // Park the pointer off the menu: a hovered row would light (and open its
  // submenu) where the other clients light none.
  await page.mouse.move(0, 0)
  // EXP-1558: on phones the menu is a bottom sheet (a dialog) that focuses
  // its first row, and the row's focus-visible fill lit "Photo or file" where
  // the natives' sheet lights none. A blur keeps the sheet open (the focus
  // trap ignores a focus that leaves for nowhere).
  await page.evaluate(() => {
    const active = document.activeElement
    if (active instanceof HTMLElement && active.getAttribute(`role`) === `menuitem`) active.blur()
  })
}

/** #EXP-1659: the same + menu with its Skills submenu open (the team's
 *  skills, the two enabled-by-default ones picked). */
async function recipeOpenComposerSkillsMenu(page: Page): Promise<void> {
  await recipeOpenComposerMenuTemporary(page)
  const row = page.getByTestId(`agent-composer-menu-skills`).filter({ visible: true }).first()
  await row.waitFor({ timeout: 15_000 })
  // md+: the submenu opens on hover and closes when the pointer leaves the
  // row, so the pointer stays on it; on phones the tap pushes its page.
  await row.hover()
  await row.click()
  await page.getByText(`incident-triage`).filter({ visible: true }).first().waitFor({ timeout: 15_000 })
}

/** #EXP-1715: the + menu's Teams submenu on the Assistant (the agent row's
 *  default pick), the last team unchecked so the chips show. */
async function recipeOpenComposerTeamsMenu(page: Page): Promise<void> {
  const plus = page.getByTestId(`agent-composer-plus-button`).filter({ visible: true }).first()
  await plus.waitFor({ timeout: 30_000 })
  await plus.click()
  const row = page.getByTestId(`agent-composer-menu-teams`).filter({ visible: true }).first()
  await row.waitFor({ timeout: 15_000 })
  await row.hover()
  await row.click()
  const last = page.getByRole(`option`).filter({ visible: true }).last()
  await last.waitFor({ timeout: 15_000 })
  await last.click()
}

/** EXP-1635: on a phone, Photo or file pushes Camera | Photos | File. */
async function recipeOpenComposerAddFile(page: Page): Promise<void> {
  const plus = page.getByTestId(`agent-composer-plus-button`).filter({ visible: true }).first()
  await plus.waitFor({ timeout: 30_000 })
  await plus.click()
  await page.getByTestId(`agent-composer-menu-add-file`).filter({ visible: true }).first().click()
  await page.getByTestId(`agent-composer-menu-add-file-camera`).first().waitFor({ timeout: 15_000 })
  await page.mouse.move(0, 0)
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())
}

/** EXP-1635: `/share` goes home with nothing pending, so a share is written
 *  where the service worker stashes one (keys: `public/sw.js`). */
async function recipeOpenShareTargets(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const swatch = async (color: string) => {
      const canvas = new OffscreenCanvas(96, 96)
      const context = canvas.getContext(`2d`)
      if (context) {
        context.fillStyle = color
        context.fillRect(0, 0, 96, 96)
      }
      return canvas.convertToBlob({ type: `image/png` })
    }
    const cache = await caches.open(`exponential-share`)
    await cache.put(`/__share/file/0`, new Response(await swatch(`#3b82f6`)))
    await cache.put(`/__share/file/1`, new Response(await swatch(`#f59e0b`)))
    await cache.put(
      `/__share/meta`,
      Response.json({
        title: ``,
        text: `Crash on launch`,
        url: ``,
        files: [
          { name: `crash-1.png`, type: `image/png`, lastModified: 0 },
          { name: `crash-2.png`, type: `image/png`, lastModified: 0 },
        ],
      })
    )
  })
  await page.goto(new URL(`/share`, page.url()).href)
  await page.getByTestId(`share-targets-preview`).waitFor({ timeout: 20_000 })
  await page.mouse.move(0, 0)
}

/** NEW-2: the home composer's `/` menu — the team's prompts. */
async function recipeTypeSlashInComposer(page: Page): Promise<void> {
  const field = page.getByTestId(`agent-composer-field`).filter({ visible: true }).first()
  await field.waitFor({ timeout: 30_000 })
  // Focus, never click: the composer's decorations can sit over the field.
  await field.focus()
  // EXP-1551: park the pointer first: the menu opens under wherever it was
  // and a hovered row steals the lit row from the first one.
  await page.mouse.move(0, 0)
  await page.keyboard.type(`/`)
  await page.getByTestId(`slash-menu`).first().waitFor({ timeout: 15_000 })
}

/** NEW-2: a live session's `/` menu — the builtins, then the prompts. */
async function recipeTypeSlashInSessionComposer(page: Page): Promise<void> {
  // Phones reply through the floating capsule (`floating-composer-capsule`):
  // tap it open first. md+ keeps the in-flow composer (`chat-composer`)
  // under a long conversation: bring it into the frame, then focus (never
  // click: the composer's decorations can sit over the field).
  const capsule = page.getByTestId(`floating-composer-capsule`).first()
  const phone = await capsule.waitFor({ timeout: 5_000 }).then(() => true, () => false)
  if (phone) await capsule.click()
  // NEW-6: a chat with ONE session shows that session's view in place, so
  // its box is the session composer (`session-composer`), the chat's own
  // (`chat-composer`) only once a second session exists.
  const field = page
    .getByTestId(/^(chat|session)-composer$/)
    .locator(`textarea`)
    .first()
  await field.waitFor({ timeout: 30_000 })
  await field.scrollIntoViewIfNeeded()
  await field.focus()
  await page.keyboard.type(`/`)
  await page.getByTestId(`slash-menu`).first().waitFor({ timeout: 15_000 })
}

/**
 * Bring one transcript card to the middle of the frame. The transcript is
 * bottom-anchored, so a card above the fold needs the scroll; centring it
 * keeps the rows around it (what the card sits in) in the shot.
 */
async function centreCard(page: Page, testId: string): Promise<void> {
  const card = page.getByTestId(testId).filter({ visible: true }).first()
  await card.waitFor({ timeout: 30_000 })
  await card.evaluate((node) => node.scrollIntoView({ block: `center` }))
  await page.waitForTimeout(400)
}

/** The raw session view's Work card (EXP-1240, work-card.json), centred in
 *  the feed. EXP-1324: subagents are Trace lines and the task list is THE
 *  Queue, so the one Work card left in a transcript is a workflow's (the
 *  demo feed's `cold-start-review`). EXP-1293: the feed always shows the
 *  work, so there is no Show work toggle to flip. */
async function recipeScrollToWorkCards(page: Page): Promise<void> {
  await centreCard(page, `work-card`)
}

/**
 * EXP-1558: the raw session view at its LATEST turn, the frame the natives
 * and desktop photograph (`sg_session`, `session:$session`): the settled
 * Trace lines, the agent's last message, the waiting question card inline
 * (ask-dock.json: answerable in the transcript) and the Queue over the
 * composer. The feed is published by the relay stub and lands after the
 * header, so the recipe waits for the question card's options, then pins the
 * transcript's scroller to its end (absolute, like `scrollToComments`), and
 * again once late rows have laid out.
 */
async function recipeScrollToLatestTurn(page: Page): Promise<void> {
  const option = page.getByTestId(`ask-option`).filter({ visible: true }).last()
  if (!(await appears(option, 45_000))) {
    throw new Error(
      `no waiting question card in the session feed: the relay stub ` +
        `(bun run screenshots:desktop) publishes it; "No live transcript" = ` +
        `the stub is not serving this session`
    )
  }
  await scrollToEnd(option)
  await page.waitForTimeout(600)
  await scrollToEnd(option)
  await page.waitForTimeout(400)
}

/** Scroll every scrolling ancestor of `target` to its end (the first that
 *  actually moves wins), else the window: an absolute resting point. */
async function scrollToEnd(target: Locator): Promise<void> {
  await target.evaluate((node) => {
    for (let el = node.parentElement; el; el = el.parentElement) {
      if (el.scrollHeight <= el.clientHeight + 1) continue
      const before = el.scrollTop
      el.scrollTop = el.scrollHeight
      if (el.scrollTop !== before || el.scrollTop > 0) return
    }
    window.scrollTo(0, document.documentElement.scrollHeight)
  })
}

/** The feed at the code-mode call's Script card (EXP-1279). */
async function recipeScrollToScriptCard(page: Page): Promise<void> {
  await centreCard(page, `script-card`)
}

/** The feed at the agent's comment, drawn as the comment (EXP-1241). */
async function recipeScrollToCommentCard(page: Page): Promise<void> {
  await centreCard(page, `comment-call-row`)
}

// --------------------------------------------------------------- machines

/** #EXP-1643: the catalog's Devices shot opens the first login's account
 *  panel in its card. */
async function recipePreviewDeviceLimits(page: Page): Promise<void> {
  const row = page.locator(`[data-testid^="device-row-"]`).first()
  if (!(await appears(row, 30_000))) {
    throw new Error(
      `no device row under "My devices": run bun run screenshots:desktop ` +
        `(and set STEER_RELAY_URL)`
    )
  }
  const chip = page.locator(`[data-testid^="device-login-chip-"]`).first()
  if (!(await appears(chip, 15_000))) return
  // A lone login opens by itself (`openDeviceAccount`): never close it.
  if ((await chip.getAttribute(`aria-expanded`)) !== `true`) await chip.click()
  await page
    .locator(`[data-slot="device-account-panel"]`)
    .first()
    .waitFor({ timeout: 3_000 })
    .catch(() => undefined)
}

/**
 * Open a device's Device settings dialog from the Devices page. The entry
 * only exists for a REGISTERED device of the caller's own (my-machines.tsx),
 * so with no relay stub running there is no card and nothing to click.
 */
async function recipeOpenMachineSettings(page: Page): Promise<void> {
  // EXP-1560: the card's ⋯ overflow carries "Device settings".
  const more = page.locator(`[data-testid^="device-menu-"]`)
  if (!(await appears(more, 30_000))) {
    throw new Error(
      `no device card ⋯ under "My devices" — the card needs a ` +
        `REGISTERED device of your own: run bun run screenshots:desktop (and ` +
        `set STEER_RELAY_URL)`
    )
  }
  await more.first().click()
  await page.getByRole(`menuitem`, { name: `Device settings` }).first().click()
  await page.getByRole(`heading`, { name: `Device settings` }).waitFor({ timeout: 15_000 })
}

/**
 * Open the "Add device" dialog — desktop download plus the CLI install
 * one-liner. The section header's trailing button is always mounted (no device
 * or relay needed), so the wait is on the snippet itself: the dialog's own
 * heading appears a frame before the `pre` is laid out, and the snippet IS the
 * view.
 *
 * The trigger's label is the dialog's title too (`my-machines.tsx`), so this
 * matches the BUTTON role exactly rather than the text — and the second wait
 * has to be dialog-only content for the same reason (EXP-697 renamed both from
 * "Add server", which stranded this recipe until 2026-09-01).
 */
async function recipeOpenAddServer(page: Page): Promise<void> {
  const trigger = page.getByRole(`button`, { name: `Add device`, exact: true })
  await trigger.first().waitFor({ timeout: 20_000 })
  await trigger.first().click()
  await page.getByText(/EXP_INSTANCE=/).first().waitFor({ timeout: 15_000 })
}

// ---------------------------------------------------------------- actions
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
 * Open the Getting started block from its sidebar-footer entry (EXP-88), which
 * EXP-1270 turned into a centred dialog over the page (the right sheet is
 * gone). EXP-548 hides the entry once every step is done, and while the
 * signals are still loading, so an absent entry is a seed problem, not a
 * flake, and says so. Both ids are getting-started.json's `testIds`.
 */
async function recipeOpenGettingStarted(page: Page): Promise<void> {
  const trigger = page.getByTestId(`getting-started-entry`)
  if (!(await appears(trigger, 15_000))) {
    await openSidebar(page)
  }
  if (!(await appears(trigger, 10_000))) {
    throw new Error(
      `no "Getting started" sidebar entry: EXP-548 hides it once every ` +
        `step is complete. Re-seed so at least one stays open: ` +
        `bun run seed:screenshots`
    )
  }
  await trigger.first().click()
  await page
    .getByRole(`dialog`)
    .getByTestId(`getting-started`)
    .waitFor({ timeout: 15_000 })
}

/**
 * The phone web's Getting started entry (#EXP-1632): the Team page's
 * `Getting started` row (the natives' Settings row) opens the same block, in
 * the bottom sheet DialogContent becomes at phone width. Runs on
 * `/t/$teamSlug/team`; the row carries getting-started.json's `entry` id and
 * hides once every step is done, like the sidebar entry.
 */
async function recipeOpenGettingStartedPhone(page: Page): Promise<void> {
  const trigger = page.getByTestId(`getting-started-entry`).filter({ visible: true })
  if (!(await appears(trigger, 15_000))) {
    throw new Error(
      `no "Getting started" row on the Team page: EXP-548 hides it once every ` +
        `step is complete. Re-seed so at least one stays open: ` +
        `bun run seed:screenshots`
    )
  }
  await trigger.first().scrollIntoViewIfNeeded()
  await trigger.first().click()
  await page
    .getByRole(`dialog`)
    .getByTestId(`getting-started`)
    .waitFor({ timeout: 15_000 })
}

/**
 * Settings → Boards lists the team's boards since EXP-1383 (each row opens
 * its own page); the `settings-boards` view photographs ONE board's page, so
 * open the first row (`boards-section.tsx` `boards-list-row-{id}`).
 */
async function recipeOpenFirstBoardSettings(page: Page): Promise<void> {
  const rows = page.locator(`[data-testid^="boards-list-row-"]`)
  await rows.first().waitFor({ timeout: 20_000 })
  await rows.first().click()
}

/**
 * Settings → Assistant's Meeting mode block (meeting.json): Meeting mode and
 * Stop after (and Desktop audio, with a voice device) sit below the fold under
 * the Voice rows, so centre the Stop after row by its label.
 */
async function recipeScrollToMeetingMode(page: Page): Promise<void> {
  const target = page.getByText(`Stop after`, { exact: true }).filter({ visible: true }).first()
  await target.waitFor({ timeout: 20_000 })
  await target.evaluate((el) => el.scrollIntoView({ block: `center` }))
}

/**
 * EXP-1558: Settings → MCP → Activity loads its `mcp_calls` over tRPC after
 * the section header paints, so the header alone photographed the skeleton
 * rows. Wait for the settled list: a call row, or the empty state when the
 * instance has no calls (settings-mcp.json `activityPage`).
 */
async function recipeWaitForMcpActivity(page: Page): Promise<void> {
  const root = page.getByTestId(`settings-mcp-activity`).filter({ visible: true }).first()
  await root.waitFor({ timeout: 20_000 })
  await root
    .getByTestId(`mcp-activity-row`)
    .or(root.getByText(emptyStateCopy(`mcp-activity`).title, { exact: true }))
    .first()
    .waitFor({ timeout: 20_000 })
}

/** EXP-1558: recipes whose photographed state IS a hover; the capturer
 *  parks the pointer after every other recipe. */
// #EXP-1659: openComposerSkillsMenu: a hover holds the Skills submenu open.
export const KEEPS_POINTER: ReadonlySet<string> = new Set([`previewDeviceLimits`, `openComposerSkillsMenu`, `openComposerTeamsMenu`])

/**
 * The registry itself. Keys are the names `views.json` uses; the view-catalog
 * test parses this literal, so keep it a flat name-to-function table with the
 * closing brace in column 0.
 */
export const RECIPES: Record<string, Recipe> = {
  openOnboardingCreateTeam: recipeOpenOnboardingCreateTeam,
  openAppPreviewTab: recipeOpenAppPreviewTab,
  openAppExport: recipeOpenAppExport,
  openAppImport: recipeOpenAppImport,
  openRecordForm: recipeOpenRecordForm,
  openRecordHistory: recipeOpenRecordHistory,
  openRecordUndo: recipeOpenRecordUndo,
  openCsvImport: recipeOpenCsvImport,
  openAppVersionPreview: recipeOpenAppVersionPreview,
  scrollAppSettingsEnd: recipeScrollAppSettingsEnd,
  openAppConsent: recipeOpenAppConsent,
  openOnboardingJoin: recipeOpenOnboardingJoin,
  openOnboardingInvite: recipeOpenOnboardingInvite,
  openOnboardingDevices: recipeOpenOnboardingDevices,
  scrollToComments: recipeScrollToComments,
  openSearch: recipeOpenSearch,
  openBoardBulkEdit: recipeOpenBoardBulkEdit,
  openChatSelection: recipeOpenChatSelection,
  openFirstBoardSettings: recipeOpenFirstBoardSettings,
  openIssuePropertiesMobile: recipeOpenIssuePropertiesMobile,
  expandFirstDiffFile: recipeExpandFirstDiffFile,
  openMachineSettings: recipeOpenMachineSettings,
  previewDeviceLimits: recipePreviewDeviceLimits,
  openAddServer: recipeOpenAddServer,
  openWidgetEditor: recipeOpenWidgetEditor,
  openGettingStarted: recipeOpenGettingStarted,
  openGettingStartedPhone: recipeOpenGettingStartedPhone,
  typeIssueRef: recipeTypeIssueRef,
  openComposerMenuTemporary: recipeOpenComposerMenuTemporary,
  openComposerSkillsMenu: recipeOpenComposerSkillsMenu,
  openComposerTeamsMenu: recipeOpenComposerTeamsMenu,
  openComposerAddFile: recipeOpenComposerAddFile,
  openShareTargets: recipeOpenShareTargets,
  typeSlashInComposer: recipeTypeSlashInComposer,
  typeSlashInSessionComposer: recipeTypeSlashInSessionComposer,
  scrollToWorkCards: recipeScrollToWorkCards,
  scrollToScriptCard: recipeScrollToScriptCard,
  scrollToLatestTurn: recipeScrollToLatestTurn,
  scrollToCommentCard: recipeScrollToCommentCard,
  scrollToMeetingMode: recipeScrollToMeetingMode,
  waitForMcpActivity: recipeWaitForMcpActivity,
}
