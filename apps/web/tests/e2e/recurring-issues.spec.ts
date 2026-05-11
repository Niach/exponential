import type { Page } from "@playwright/test"
import { and, eq } from "drizzle-orm"
import { registerUser } from "./helpers/auth"
import { db } from "../../src/db/connection"
import { users } from "../../src/db/auth-schema"
import { issues, projects, workspaces } from "../../src/db/schema"
import { expect, test, type AppFixture } from "./fixtures"

// ─── local helpers ──────────────────────────────────────────────────────────

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

function getWorkspaceSlug(url: string) {
  const [, , slug] = new URL(url).pathname.split(`/`)
  return slug
}

function isoDateOffset(offsetDays: number): string {
  const d = new Date()
  d.setUTCDate(d.getUTCDate() + offsetDays)
  return d.toISOString().slice(0, 10)
}

async function seedTodoIssues(
  workspaceSlug: string,
  projectSlug: string,
  creatorEmail: string,
  rows: Array<{ title: string; dueDate?: string }>
) {
  const [creator] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, creatorEmail))
  if (!creator) throw new Error(`Creator not found: ${creatorEmail}`)

  const [workspace] = await db
    .select({ id: workspaces.id })
    .from(workspaces)
    .where(eq(workspaces.slug, workspaceSlug))
  if (!workspace) throw new Error(`Workspace not found: ${workspaceSlug}`)

  const [project] = await db
    .select({ id: projects.id })
    .from(projects)
    .where(
      and(
        eq(projects.workspaceId, workspace.id),
        eq(projects.slug, projectSlug)
      )
    )
  if (!project) throw new Error(`Project not found: ${projectSlug}`)

  for (const row of rows) {
    await db.insert(issues).values({
      projectId: project.id,
      title: row.title,
      status: `todo`,
      priority: `none`,
      creatorId: creator.id,
      dueDate: row.dueDate ?? null,
    })
  }
}

// ─── tests ──────────────────────────────────────────────────────────────────

test(`create recurring issue shows Repeat icon in list`, async ({
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

  // Enable recurrence via the overflow menu
  await createDialog.getByRole(`button`, { name: `More options` }).click()
  await page.getByRole(`menuitem`, { name: `Make recurring…` }).click()

  // Recurrence footer should be active
  await expect(
    createDialog.getByRole(`button`, { name: `Create recurring issue` })
  ).toBeVisible()
  // Inline due-date chip is hidden when recurrence is active
  await expect(
    createDialog.getByRole(`button`, { name: `Due date` })
  ).toBeHidden()

  // Pick today as first-due date (computed inside the browser to match its timezone)
  await createDialog.getByRole(`button`, { name: /Pick date/ }).click()
  const todayDataDay = await page.evaluate(() =>
    new Date().toLocaleDateString(`en-US`)
  )
  await page
    .locator(`[data-slot="calendar"] [data-day="${todayDataDay}"]`)
    .last()
    .click()
  // Dismiss calendar by clicking elsewhere
  await createDialog.getByPlaceholder(`Issue title`).click()

  // Set interval to 2 (first combobox in the RecurrenceEditor)
  await createDialog.getByRole(`combobox`).first().click()
  await page.getByRole(`option`, { name: `2` }).click()

  // Set unit to "days" (second combobox; shows plural labels when interval > 1)
  await createDialog.getByRole(`combobox`).last().click()
  await page.getByRole(`option`, { name: `days` }).click()

  await createDialog.getByPlaceholder(`Issue title`).fill(`Laundry`)
  await createDialog.getByRole(`button`, { name: `Create recurring issue` }).click()
  await expect(createDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  const todoGroup = page.locator(`[data-testid="issue-group-todo"]`)
  const laundryRow = todoGroup
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })

  await expect(laundryRow).toBeVisible()
  // Repeat icon has aria-label="Recurring" (see issue-list.tsx)
  await expect(laundryRow.locator(`[aria-label="Recurring"]`)).toBeVisible()
})

test(`completing a recurring issue spawns a new todo instance; non-recurring does not`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  // ── Create the recurring issue ────────────────────────────────────────────
  await page.getByRole(`button`, { name: `New Issue` }).click()
  const createDialog = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog).toBeVisible()

  await createDialog.getByRole(`button`, { name: `More options` }).click()
  await page.getByRole(`menuitem`, { name: `Make recurring…` }).click()

  await createDialog.getByRole(`button`, { name: /Pick date/ }).click()
  const todayDataDay = await page.evaluate(() =>
    new Date().toLocaleDateString(`en-US`)
  )
  await page
    .locator(`[data-slot="calendar"] [data-day="${todayDataDay}"]`)
    .last()
    .click()
  await createDialog.getByPlaceholder(`Issue title`).click()

  await createDialog.getByRole(`combobox`).first().click()
  await page.getByRole(`option`, { name: `2` }).click()

  await createDialog.getByRole(`combobox`).last().click()
  await page.getByRole(`option`, { name: `days` }).click()

  await createDialog.getByPlaceholder(`Issue title`).fill(`Laundry`)
  await createDialog.getByRole(`button`, { name: `Create recurring issue` }).click()
  await expect(createDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // ── Capture the original issue's identifier ───────────────────────────────
  const todoGroup = page.locator(`[data-testid="issue-group-todo"]`)
  const laundryRow = todoGroup
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })
  await expect(laundryRow).toBeVisible()

  const identifier = (
    await laundryRow.locator(`span.font-mono`).textContent()
  )?.trim()
  if (!identifier) throw new Error(`Could not read issue identifier`)

  // ── Mark the original done via the edit dialog ────────────────────────────
  const originalRow = page.locator(`[data-testid="issue-row-${identifier}"]`)
  await originalRow.click()

  const editDialog = page.locator(`[data-testid="issue-editor-edit"]`)
  await expect(editDialog).toBeVisible()

  await editDialog.getByRole(`button`, { name: `Todo` }).click()
  await page.getByRole(`menuitem`, { name: `Done` }).click()
  await editDialog.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Original moved to Done group
  await expect(
    page.locator(
      `[data-testid="issue-group-done"] [data-testid="issue-row-${identifier}"]`
    )
  ).toBeVisible()

  // A new Laundry issue appears in Todo with the Repeat icon (the clone)
  const cloneRow = page
    .locator(`[data-testid="issue-group-todo"] [data-testid^="issue-row-"]`)
    .filter({ hasText: `Laundry` })
  await expect(cloneRow).toBeVisible()
  await expect(cloneRow.locator(`[aria-label="Recurring"]`)).toBeVisible()

  // ── Negative: completing a non-recurring issue does not spawn a clone ──────
  await page.getByRole(`button`, { name: `New Issue` }).click()
  const createDialog2 = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog2).toBeVisible()

  await createDialog2.getByPlaceholder(`Issue title`).fill(`One-shot task`)
  await createDialog2.getByRole(`button`, { name: `Create issue` }).click()
  await expect(createDialog2).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Non-recurring issue starts in Backlog; find it across all rows
  const oneShotRow = page
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: `One-shot task` })
  await expect(oneShotRow).toHaveCount(1)

  const oneShotIdentifier = (
    await oneShotRow.locator(`span.font-mono`).textContent()
  )?.trim()
  if (!oneShotIdentifier) throw new Error(`Could not read one-shot identifier`)

  await oneShotRow.click()
  const editDialog2 = page.locator(`[data-testid="issue-editor-edit"]`)
  await expect(editDialog2).toBeVisible()

  await editDialog2.getByRole(`button`, { name: `Backlog` }).click()
  await page.getByRole(`menuitem`, { name: `Done` }).click()
  await editDialog2.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog2).toBeHidden()

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  // Exactly one row with "One-shot task" — no clone was created
  await expect(
    page
      .locator(`[data-testid^="issue-row-"]`)
      .filter({ hasText: `One-shot task` })
  ).toHaveCount(1)
})

test(`overdue issues sort before future issues within a status group`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )

  const workspaceSlug = getWorkspaceSlug(page.url())

  // Seed issues in arbitrary insertion order (sort must be by date, not insertion order)
  await seedTodoIssues(workspaceSlug, app.projectSlug, app.owner.email, [
    { title: `Tomorrow issue`, dueDate: isoDateOffset(1) },
    { title: `No-date issue` },
    { title: `Today issue`, dueDate: isoDateOffset(0) },
    { title: `Yesterday issue`, dueDate: isoDateOffset(-1) },
  ])

  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  const todoGroup = page.locator(`[data-testid="issue-group-todo"]`)
  const rows = todoGroup.locator(`[data-testid^="issue-row-"]`)

  await expect(rows).toHaveCount(4)

  // Assert positional order: yesterday < today < tomorrow < (no date)
  await expect(rows.nth(0)).toContainText(`Yesterday issue`)
  await expect(rows.nth(1)).toContainText(`Today issue`)
  await expect(rows.nth(2)).toContainText(`Tomorrow issue`)
  await expect(rows.nth(3)).toContainText(`No-date issue`)
})
