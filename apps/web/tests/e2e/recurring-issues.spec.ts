import type { Page } from "@playwright/test"
import { eq } from "drizzle-orm"
import { registerUser } from "./helpers/auth"
import { db } from "../../../src/db/connection"
import { issues, projects } from "../../../src/db/schema"
import { users } from "../../../src/db/auth-schema"
import { expect, test, type AppFixture } from "./fixtures"

async function createProject(page: Page, app: AppFixture) {
  await page.getByLabel(`Create project`).click()

  const dialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Create project` }),
  })

  await expect(dialog).toBeVisible()
  await dialog.getByLabel(`Name`).fill(app.projectName)
  await expect(dialog.getByLabel(`Prefix`)).toHaveValue(app.projectPrefix)
  await dialog.getByRole(`button`, { name: `Create project` }).click()
  await expect(dialog).toBeHidden()
}

function formatDateOnly(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, `0`)
  const day = String(date.getDate()).padStart(2, `0`)
  return `${year}-${month}-${day}`
}

test(`creates a recurring issue and shows the Repeat icon in the list`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  await page.getByRole(`button`, { name: `New Issue` }).click()

  const createDialog = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog).toBeVisible()

  // Open the overflow menu and enable recurrence
  await createDialog.getByRole(`button`, { name: `More options` }).click()
  await page.getByRole(`menuitem`, { name: `Make recurring…` }).click()

  // Footer swaps: submit button becomes "Create recurring issue"
  await expect(
    createDialog.getByRole(`button`, { name: `Create recurring issue` })
  ).toBeVisible()

  // Inline due-date chip in the chip row must be hidden when recurrence is active
  await expect(
    createDialog.getByRole(`button`, { name: `Due date` })
  ).toBeHidden()

  // Pick a first-due date in the RecurrenceEditor calendar
  await createDialog
    .getByRole(`button`, { name: /^(Pick date|[A-Z][a-z]{2} \d{1,2})$/ })
    .click()
  await page
    .locator(`[data-slot="calendar"] [data-day="${app.dueDate.dataDay}"]`)
    .last()
    .click()
  // Dismiss calendar by clicking the title field
  await createDialog.getByPlaceholder(`Issue title`).click()

  // Set interval to 2 (first combobox in the dialog is the interval Select)
  await createDialog.getByRole(`combobox`).first().click()
  await page.getByRole(`option`, { name: `2` }).click()

  // Set unit to 'day' — with interval=2 the option label is "days"
  // (second combobox is the unit Select)
  await createDialog.getByRole(`combobox`).last().click()
  await page.getByRole(`option`, { name: `days` }).click()

  // Submit
  await createDialog.getByPlaceholder(`Issue title`).fill(`Laundry`)
  await createDialog
    .getByRole(`button`, { name: `Create recurring issue` })
    .click()
  await expect(createDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  const laundryRow = page
    .locator(`[data-testid="issue-group-todo"]`)
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })

  await expect(laundryRow).toHaveCount(1)
  await expect(laundryRow.getByLabel(`Recurring`)).toBeVisible()
})

test(`marks a recurring issue done and spawns a clone in Todo`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  // Create the recurring issue (interval=2, unit=day)
  await page.getByRole(`button`, { name: `New Issue` }).click()
  const createDialog = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog).toBeVisible()
  await createDialog.getByRole(`button`, { name: `More options` }).click()
  await page.getByRole(`menuitem`, { name: `Make recurring…` }).click()
  await expect(
    createDialog.getByRole(`button`, { name: `Create recurring issue` })
  ).toBeVisible()
  await createDialog.getByRole(`combobox`).first().click()
  await page.getByRole(`option`, { name: `2` }).click()
  await createDialog.getByRole(`combobox`).last().click()
  await page.getByRole(`option`, { name: `days` }).click()
  await createDialog.getByPlaceholder(`Issue title`).fill(`Laundry`)
  await createDialog
    .getByRole(`button`, { name: `Create recurring issue` })
    .click()
  await expect(createDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Open the issue and mark it Done
  const laundryRow = page
    .locator(`[data-testid="issue-group-todo"]`)
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })
  await expect(laundryRow).toHaveCount(1)
  await laundryRow.click()

  const editDialog = page.locator(`[data-testid="issue-editor-edit"]`)
  await expect(editDialog).toBeVisible()
  await editDialog.getByRole(`button`, { name: `Todo` }).click()
  await page.getByRole(`menuitem`, { name: `Done` }).click()
  await editDialog.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Original is now in Done group
  await expect(
    page
      .locator(`[data-testid="issue-group-done"]`)
      .locator(`[data-testid^="issue-row-"]`)
      .filter({ hasText: `Laundry` })
  ).toHaveCount(1)

  // Clone appears in Todo group with Repeat icon and due date = today + 2 days
  const today = new Date()
  const cloneDate = new Date(
    today.getFullYear(),
    today.getMonth(),
    today.getDate() + 2
  )
  const cloneDueDateText = cloneDate.toLocaleDateString(`en-US`, {
    month: `short`,
    day: `numeric`,
  })

  const cloneRow = page
    .locator(`[data-testid="issue-group-todo"]`)
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })

  await expect(cloneRow).toHaveCount(1)
  await expect(cloneRow.getByLabel(`Recurring`)).toBeVisible()
  await expect(cloneRow).toContainText(cloneDueDateText)
})

test(`does not spawn a clone when a non-recurring issue is marked done`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  // Create a plain (non-recurring) issue
  await page.getByRole(`button`, { name: `New Issue` }).click()
  const createDialog = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog).toBeVisible()
  await createDialog.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  await createDialog.getByRole(`button`, { name: `Create issue` }).click()
  await expect(createDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Open and mark Done
  const issueRow = page
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: app.issueTitle })
  await expect(issueRow).toHaveCount(1)
  await issueRow.click()

  const editDialog = page.locator(`[data-testid="issue-editor-edit"]`)
  await expect(editDialog).toBeVisible()
  await editDialog.getByRole(`button`, { name: `Backlog` }).click()
  await page.getByRole(`menuitem`, { name: `Done` }).click()
  await editDialog.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Exactly one row — in Done, not in Todo (no clone)
  await expect(
    page
      .locator(`[data-testid="issue-group-done"]`)
      .locator(`[data-testid^="issue-row-"]`)
      .filter({ hasText: app.issueTitle })
  ).toHaveCount(1)

  await expect(
    page
      .locator(`[data-testid="issue-group-todo"]`)
      .locator(`[data-testid^="issue-row-"]`)
      .filter({ hasText: app.issueTitle })
  ).toHaveCount(0)
})

test(`shows overdue issues first within the Todo status group`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  // Resolve the creator and project IDs from the DB
  const [ownerRow] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, app.owner.email))

  const [projectRow] = await db
    .select({ id: projects.id })
    .from(projects)
    .where(eq(projects.slug, app.projectSlug))
    .limit(1)

  if (!ownerRow || !projectRow) {
    throw new Error(`Could not find owner or project in DB`)
  }

  const today = new Date()
  const yesterday = new Date(
    today.getFullYear(),
    today.getMonth(),
    today.getDate() - 1
  )
  const tomorrow = new Date(
    today.getFullYear(),
    today.getMonth(),
    today.getDate() + 1
  )

  // Seed 4 issues directly into the DB with different due dates
  await db.insert(issues).values([
    {
      projectId: projectRow.id,
      title: `Yesterday Task`,
      status: `todo`,
      priority: `none`,
      dueDate: formatDateOnly(yesterday),
      creatorId: ownerRow.id,
    },
    {
      projectId: projectRow.id,
      title: `Today Task`,
      status: `todo`,
      priority: `none`,
      dueDate: formatDateOnly(today),
      creatorId: ownerRow.id,
    },
    {
      projectId: projectRow.id,
      title: `Tomorrow Task`,
      status: `todo`,
      priority: `none`,
      dueDate: formatDateOnly(tomorrow),
      creatorId: ownerRow.id,
    },
    {
      projectId: projectRow.id,
      title: `No Due Date Task`,
      status: `todo`,
      priority: `none`,
      creatorId: ownerRow.id,
    },
  ])

  // Reload so Electric picks up the newly seeded issues
  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  const todoGroup = page.locator(`[data-testid="issue-group-todo"]`)
  const rows = todoGroup.locator(`[data-testid^="issue-row-"]`)

  // Wait for all 4 issues to appear
  await expect(rows).toHaveCount(4)

  // Sort order: overdue (yesterday) first, then by dueDate asc, then no-dueDate last
  await expect(rows.nth(0)).toContainText(`Yesterday Task`)
  await expect(rows.nth(1)).toContainText(`Today Task`)
  await expect(rows.nth(2)).toContainText(`Tomorrow Task`)
  await expect(rows.nth(3)).toContainText(`No Due Date Task`)
})
