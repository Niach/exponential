import type { Locator, Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

const SELECT_ALL_SHORTCUT = process.platform === `darwin` ? `Meta+A` : `Control+A`

async function createProject(page: Page, app: AppFixture) {
  await page.getByLabel(`Create project`).click()

  const dialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Create project` }),
  })

  await dialog.getByLabel(`Name`).fill(app.projectName)
  await expect(dialog.getByLabel(`Prefix`)).toHaveValue(app.projectPrefix)
  await dialog.getByRole(`button`, { name: `Create project` }).click()
  await expect(dialog).toBeHidden()
}

async function replaceIssueDescription(page: Page, dialog: Locator, text: string) {
  const editor = dialog.getByLabel(`Issue description`)
  await editor.click()
  await editor.press(SELECT_ALL_SHORTCUT)
  await page.keyboard.type(text)
}

async function selectDueDate(page: Page, dialog: Locator, dataDay: string) {
  await dialog.getByRole(`button`, { name: /Due date|^[A-Z][a-z]{2} \d{1,2}$/ }).click()
  const calendar = page.locator(`[data-slot="calendar"]`).last()

  await page.locator(`[data-slot="calendar"] [data-day="${dataDay}"]`).last().click()
  await dialog.getByPlaceholder(`Issue title`).click()
  await expect(calendar).toBeHidden()
}

test(`creates and edits an issue through the shared issue editor`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)

  await expect(page).toHaveURL(new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`))

  await page.getByRole(`button`, { name: `New Issue` }).click()

  const createDialog = page.locator(`[data-testid="issue-editor-create"]`)
  await expect(createDialog).toBeVisible()

  await createDialog.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  await replaceIssueDescription(page, createDialog, app.issueDescription)

  await createDialog.getByRole(`button`, { name: `Label` }).click()
  await page.getByText(`Create label`).click()
  await page.getByPlaceholder(`Label name`).fill(app.labelName)
  await page.getByLabel(`Select label color ${app.labelColor}`).click()
  await page.getByRole(`button`, { name: `Create label` }).click()
  await page.keyboard.press(`Escape`)

  await createDialog.getByRole(`button`, { name: `Assignee` }).click()
  await page.getByText(app.owner.name, { exact: true }).click()

  await createDialog.getByRole(`button`, { name: `No priority` }).click()
  await page.getByRole(`menuitem`, { name: `High` }).click()

  await selectDueDate(page, createDialog, app.dueDate.dataDay)

  await createDialog.getByRole(`button`, { name: `Create issue` }).click()
  await expect(createDialog).toBeHidden()
  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  const createdRow = page
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: app.issueTitle })

  await expect(createdRow).toHaveCount(1)
  await expect(createdRow).toBeVisible()
  await expect(createdRow).toContainText(app.labelName)
  await expect(createdRow).toContainText(app.dueDate.text)
  await expect(createdRow).toContainText(app.owner.initials)

  const identifier = (await createdRow.locator(`span.font-mono`).textContent())?.trim()

  if (!identifier) {
    throw new Error(`Expected a generated issue identifier after issue creation.`)
  }

  const row = page.locator(`[data-testid="issue-row-${identifier}"]`)
  await expect(row.locator(`span.font-mono`)).toHaveText(identifier)

  await row.click()

  const editDialog = page.locator(`[data-testid="issue-editor-edit"]`)
  await expect(editDialog).toBeVisible()
  await expect(editDialog.getByRole(`button`, { name: app.labelName })).toBeVisible()

  const titleInput = editDialog.getByPlaceholder(`Issue title`)
  await titleInput.fill(app.updatedIssueTitle)
  await editDialog.getByLabel(`Issue description`).click()
  await replaceIssueDescription(page, editDialog, app.updatedIssueDescription)

  await editDialog.getByRole(`button`, { name: `Backlog` }).click()
  await page.getByRole(`menuitem`, { name: `In Progress` }).click()

  await editDialog.getByRole(`button`, { name: `High` }).click()
  await page.getByRole(`menuitem`, { name: `Urgent` }).click()

  await editDialog.getByRole(`button`, { name: app.labelName }).click()
  await page.getByLabel(`Suggestions`).getByText(app.labelName, { exact: true }).click()
  await page.keyboard.press(`Escape`)

  await editDialog.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog).toBeHidden()
  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  await expect(row).toContainText(app.updatedIssueTitle)
  await expect(row).not.toContainText(app.labelName)

  await row.click()
  await expect(editDialog).toBeVisible()
  await expect(editDialog.getByPlaceholder(`Issue title`)).toHaveValue(
    app.updatedIssueTitle
  )
  await expect(editDialog.getByLabel(`Issue description`)).toContainText(
    app.updatedIssueDescription
  )
  await expect(editDialog.getByRole(`button`, { name: `In Progress` })).toBeVisible()
  await expect(editDialog.getByRole(`button`, { name: `Urgent` })).toBeVisible()
  await expect(editDialog.getByRole(`button`, { name: `Label` })).toBeVisible()

  await editDialog.getByRole(`button`, { name: `In Progress` }).click()
  await page.getByRole(`menuitem`, { name: `Done` }).click()
  await editDialog.getByRole(`button`, { name: `Close dialog` }).click()
  await expect(editDialog).toBeHidden()
  await page.reload()
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()

  await expect(
    page.locator(`[data-testid="issue-group-in_progress"] [data-testid="issue-row-${identifier}"]`)
  ).toHaveCount(0)
  await expect(
    page.locator(`[data-testid="issue-group-done"] [data-testid="issue-row-${identifier}"]`)
  ).toBeVisible()

  await page.getByRole(`button`, { name: `Backlog` }).click()
  await expect(page.locator(`[data-testid="issue-row-${identifier}"]`)).toHaveCount(0)

  await page.getByRole(`button`, { name: `All Issues` }).click()
  await expect(
    page.locator(`[data-testid="issue-group-done"] [data-testid="issue-row-${identifier}"]`)
  ).toBeVisible()
})
