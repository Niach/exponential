import type { Page } from "@playwright/test"
import { eq } from "drizzle-orm"
import { db } from "../../src/db/connection"
import { boards, issues } from "../../src/db/schema"
import { createTeamThroughOnboarding, registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

// EXP-1074 — the issue context menu's four gestures, each one a way it used
// to fold or misfire:
//
//   S1  a right-click RELEASED a few px to the right of where it was pressed
//       used to land its pointer-up on a menu item (macOS fires `contextmenu`
//       on mouse DOWN, the menu mounts beside the cursor, and a Radix item
//       selects on pointer-up) — "opens and instantly closes again";
//   S2  focus in a text field elsewhere must not matter;
//   S3  a right-click on a second row while open moves the menu there;
//   S4  a touch long-press opens it without the row's tap navigating.

// Straight into the table (the identifier trigger numbers them): the dialog
// route lands on every created issue, and the scenarios need a list that is
// taller than the viewport.
async function seedIssues(app: AppFixture, count: number) {
  const [board] = await db
    .select({ id: boards.id, teamId: boards.teamId })
    .from(boards)
    .where(eq(boards.slug, app.boardSlug))
  if (!board) throw new Error(`board ${app.boardSlug} not found`)
  await db.insert(issues).values(
    Array.from({ length: count }, (_, index) => ({
      boardId: board.id,
      teamId: board.teamId,
      title: `${app.issueTitle} ${String(index + 1).padStart(2, `0`)}`,
      status: `backlog` as const,
      priority: `none` as const,
    }))
  )
}

async function boardWithIssues(page: Page, app: AppFixture, count = 24) {
  await registerUser(page, app.owner)
  await createTeamThroughOnboarding(page, `${app.namespace} team`, app.boardName)
  await seedIssues(app, count)
  const teamSlug = new URL(page.url()).pathname.split(`/`)[2]
  await page.goto(`/t/${teamSlug}/boards/${app.boardSlug}`)
  // The first snapshot of a fresh team's shapes can take a while on a busy
  // dev stack.
  await expect(page.locator(`[data-testid^="issue-row-"]`)).toHaveCount(count, {
    timeout: 60_000,
  })
}

const rows = (page: Page) => page.locator(`[data-testid^="issue-row-"]`)
const menu = (page: Page) => page.getByRole(`menu`)

async function rowCenter(page: Page, index: number, offsetX = 140) {
  const box = await rows(page).nth(index).boundingBox()
  if (!box) throw new Error(`row ${index} has no box`)
  return { x: box.x + Math.min(offsetX, box.width * 0.8), y: box.y + box.height / 2 }
}

test.describe(`issue context menu`, () => {
  // A short viewport: the menu is taller than what is left below any row, so
  // it is pinned to the top edge and its plain items straddle the cursor for
  // the rows near the top (the reporter's 905px-high window).
  test.describe(`short window`, () => {
    test.use({ viewport: { width: 1280, height: 720 } })

  test(`S1: a right-click released over an item keeps the menu open`, async ({
    app,
    page,
  }) => {
    await page.addInitScript(() => {
      const writes: string[] = []
      ;(window as unknown as { __clipboardWrites: string[] }).__clipboardWrites =
        writes
      Object.defineProperty(navigator, `clipboard`, {
        configurable: true,
        value: {
          writeText: async (text: string) => {
            writes.push(text)
          },
        },
      })
    })
    await boardWithIssues(page, app)

    // Find a row whose right-click puts a PLAIN item (not a submenu trigger)
    // beside the cursor: the menu is taller than the space below any row in
    // the lower half, so it shifts up and its items straddle the cursor.
    const count = await rows(page).count()
    const viewport = page.viewportSize()!
    let target: { x: number; y: number } | null = null
    for (let index = 0; index < count && !target; index += 1) {
      const centre = await rowCenter(page, index)
      if (centre.y > viewport.height - 40) break
      await page.mouse.click(centre.x, centre.y, { button: `right` })
      await expect(menu(page)).toBeVisible()
      const beside = await page.evaluate(({ y }) => {
        const items = document.querySelectorAll<HTMLElement>(
          `[role="menu"] [role="menuitem"]`
        )
        for (const item of items) {
          const box = item.getBoundingClientRect()
          if (y >= box.top + 4 && y <= box.bottom - 4) {
            return {
              label: item.textContent ?? ``,
              submenu: item.getAttribute(`aria-haspopup`) === `menu`,
            }
          }
        }
        return null
      }, centre)
      await page.keyboard.press(`Escape`)
      await expect(menu(page)).toBeHidden()
      // Past the exit animation: a right-click during it is swallowed.
      await page.waitForTimeout(300)
      if (beside && !beside.submenu) target = centre
    }
    expect(target, `a row with a plain item beside the cursor`).not.toBeNull()
    const { x, y } = target!

    // Press, open on the DOWN (the macOS ordering, forced so the scenario is
    // the same on Linux), drift 12px right, release.
    await page.mouse.move(x, y)
    await page.mouse.down({ button: `right` })
    await page.evaluate(
      ({ x, y }) => {
        const el = document.elementFromPoint(x, y)
        el?.dispatchEvent(
          new MouseEvent(`contextmenu`, {
            bubbles: true,
            cancelable: true,
            clientX: x,
            clientY: y,
            button: 2,
          })
        )
      },
      { x, y }
    )
    await expect(menu(page)).toBeVisible()
    await page.mouse.move(x + 12, y, { steps: 3 })
    await page.mouse.up({ button: `right` })

    await page.waitForTimeout(500)
    await expect(menu(page)).toBeVisible()
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __clipboardWrites: string[] })
            .__clipboardWrites
      )
    ).toEqual([])
    await expect(page).toHaveURL(
      new RegExp(`/t/[^/]+/boards/${app.boardSlug}/?$`)
    )
  })

  })

  test(`S2: focus in a text field elsewhere does not close it`, async ({
    app,
    page,
  }) => {
    await boardWithIssues(page, app)
    await page.evaluate(() => {
      const input = document.createElement(`input`)
      input.id = `pw-focus-sink`
      input.style.position = `fixed`
      input.style.top = `0`
      input.style.left = `0`
      document.body.appendChild(input)
      input.focus()
    })
    const { x, y } = await rowCenter(page, 3)
    await page.mouse.click(x, y, { button: `right` })
    await expect(menu(page)).toBeVisible()
    await page.waitForTimeout(1000)
    await expect(menu(page)).toBeVisible()
  })

  test(`S3: a right-click on another row while open moves the menu there`, async ({
    app,
    page,
  }) => {
    await boardWithIssues(page, app)
    const second = rows(page).nth(5)
    const secondId = (await second.getAttribute(`data-testid`))!.replace(
      `issue-row-`,
      ``
    )
    await page.evaluate(() => {
      ;(window as unknown as { __cm: boolean[] }).__cm = []
      document.addEventListener(`contextmenu`, (event) => {
        ;(window as unknown as { __cm: boolean[] }).__cm.push(
          event.defaultPrevented
        )
      })
    })
    // A menu hangs to the RIGHT of its cursor: open the first on the title,
    // then right-click the second row's identifier, left of that menu.
    const a = await rowCenter(page, 1, 420)
    await page.mouse.click(a.x, a.y, { button: `right` })
    await expect(menu(page)).toBeVisible()
    const b = await rowCenter(page, 5, 60)
    await page.mouse.click(b.x, b.y, { button: `right` })
    await expect(menu(page)).toHaveCount(1)
    await expect(menu(page)).toContainText(secondId)
    // Every contextmenu on a row was ours (no native menu popped).
    expect(
      await page.evaluate(
        () => (window as unknown as { __cm: boolean[] }).__cm
      )
    ).toEqual([true, true])
  })

  test.describe(`touch`, () => {
    test.use({ hasTouch: true, viewport: { width: 390, height: 844 } })

    test(`S4: a long-press opens the menu and the tap does not navigate`, async ({
      app,
      page,
      context,
    }) => {
      await boardWithIssues(page, app, 6)
      const { x, y } = await rowCenter(page, 2)
      const cdp = await context.newCDPSession(page)
      await cdp.send(`Input.dispatchTouchEvent`, {
        type: `touchStart`,
        touchPoints: [{ x, y }],
      })
      await page.waitForTimeout(800)
      await cdp.send(`Input.dispatchTouchEvent`, {
        type: `touchEnd`,
        touchPoints: [],
      })
      await expect(menu(page)).toBeVisible()
      await expect(page.getByRole(`menuitem`, { name: `Select` })).toBeVisible()
      await expect(page).toHaveURL(
        new RegExp(`/t/[^/]+/boards/${app.boardSlug}/?$`)
      )
    })
  })
})
