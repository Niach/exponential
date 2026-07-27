import type { Page } from "@playwright/test"
import { createTeamThroughOnboarding, registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

// EXP-314 — custom issue statuses end to end: the settings section manages
// per-team statuses (locked builtins, started cap, reassign-on-delete), and
// the board picker + grouping follow the team's rows.

function getTeamSlug(currentUrl: string) {
  return new URL(currentUrl).pathname.split(`/`)[2]
}

async function openStatusesSettings(page: Page, teamSlug: string) {
  await page.goto(`/t/${teamSlug}/settings/statuses`)
  // CardTitle renders as a div, not a heading (shadcn slot convention).
  await expect(
    page.locator(`[data-slot="card-title"]`).filter({ hasText: `Statuses` })
  ).toBeVisible()
}

async function createStatus(
  page: Page,
  category: `Backlog` | `Unstarted` | `Started` | `Completed` | `Cancelled`,
  name: string
) {
  await page.getByRole(`button`, { name: `Add ${category} status` }).click()
  await page
    .getByPlaceholder(`New ${category.toLowerCase()} status`)
    .fill(name)
  await page.getByRole(`button`, { name: `Create status` }).click()
  // The row lands via Electric sync (its actions menu is named after it).
  await expect(
    page.getByRole(`button`, { name: `Status actions for ${name}` })
  ).toBeVisible()
}

// A custom status row, anchored by its color-swatch button's accessible name.
function statusRow(page: Page, name: string) {
  return page
    .locator(`div.rounded-md.border`)
    .filter({ has: page.getByLabel(`Change color of ${name}`) })
}

test(`manages custom statuses in settings and uses them on the board`, async ({
  app,
  page,
}: {
  app: AppFixture
  page: Page
}) => {
  await registerUser(page, app.owner)
  await createTeamThroughOnboarding(
    page,
    `${app.namespace} team`,
    app.boardName
  )
  const teamSlug = getTeamSlug(page.url())

  await openStatusesSettings(page, teamSlug)

  // The 7 locked builtins are seeded and visible; the backlog builtin is the
  // new-issue default; Duplicate is fixed (no add button); builtins expose no
  // rename input.
  for (const name of [
    `Backlog`,
    `Todo`,
    `In Progress`,
    `In Review`,
    `Done`,
    `Duplicate`,
  ]) {
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible()
  }
  await expect(page.getByText(`Default`, { exact: true })).toBeVisible()
  await expect(
    page.getByRole(`button`, { name: `Add Duplicate status` })
  ).toHaveCount(0)
  await expect(page.getByLabel(`Status name`)).toHaveCount(0)

  // Third + fourth started statuses; the started cap disables the add button.
  await createStatus(page, `Started`, `Rückfrage`)
  await createStatus(page, `Started`, `Blocked`)
  await expect(
    page.getByRole(`button`, { name: `Add Started status` })
  ).toBeDisabled()

  // Rename a custom status inline (blur persists; the row re-mounts synced).
  const rueckfrage = statusRow(page, `Rückfrage`).getByRole(`textbox`, {
    name: `Status name`,
  })
  await rueckfrage.fill(`Waiting`)
  await rueckfrage.blur()
  await expect(page.getByLabel(`Change color of Waiting`)).toBeVisible()

  // Board: the create-dialog status chip offers the custom status; the new
  // issue groups under it.
  await page.goto(`/t/${teamSlug}/boards/${app.boardSlug}`)
  await page.getByRole(`button`, { name: `New Issue`, exact: true }).click()
  const dialog = page.getByRole(`dialog`)
  await dialog.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  await dialog.getByRole(`button`, { name: `Backlog` }).click()
  await page.getByRole(`menuitem`, { name: `Waiting` }).click()
  await dialog.getByRole(`button`, { name: `Create issue` }).click()
  await expect(dialog).toBeHidden()

  const waitingGroup = page
    .locator(`[data-status-key]`)
    .filter({ has: page.getByText(`Waiting`, { exact: true }) })
  await expect(waitingGroup.getByText(app.issueTitle)).toBeVisible()

  // Deleting the custom status walks through the reassign dialog (Backlog
  // builtin preselected) and the issue survives on Backlog.
  await openStatusesSettings(page, teamSlug)
  await page.getByRole(`button`, { name: `Status actions for Waiting` }).click()
  await page.getByRole(`menuitem`, { name: `Delete` }).click()
  const reassign = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Delete Waiting?` }),
  })
  await expect(reassign).toBeVisible()
  await reassign.getByRole(`button`, { name: `Delete status` }).click()
  await expect(reassign).toBeHidden()
  await expect(
    page.getByRole(`button`, { name: `Status actions for Waiting` })
  ).toHaveCount(0)

  await page.goto(`/t/${teamSlug}/boards/${app.boardSlug}`)
  await expect(
    page.locator(`[data-status-key="backlog"]`).getByText(app.issueTitle)
  ).toBeVisible()
})
