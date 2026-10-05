import type { Locator, Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

const SELECT_ALL_SHORTCUT =
  process.platform === `darwin` ? `Meta+A` : `Control+A`
const PNG_BUFFER = Buffer.from(
  `iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4////fwAJ+wP9KobjigAAAABJRU5ErkJggg==`,
  `base64`
)

async function createBoard(page: Page, app: AppFixture) {
  await page.getByLabel(`Create board`).click()

  const dialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Create board` }),
  })

  await dialog.getByLabel(`Name`).fill(app.boardName)
  await expect(dialog.getByLabel(`Prefix`)).toHaveValue(app.boardPrefix)
  await dialog.getByRole(`button`, { name: `Create board` }).click()
  await expect(dialog).toBeHidden()
}

async function replaceIssueDescription(
  page: Page,
  scope: Locator,
  text: string
) {
  const editor = scope.getByLabel(`Issue description`)
  await editor.click()
  await editor.press(SELECT_ALL_SHORTCUT)
  await page.keyboard.type(text)
}

async function selectDueDate(page: Page, scope: Locator, dataDay: string) {
  await scope
    .getByRole(`button`, { name: /Due date|^[A-Z][a-z]{2} \d{1,2}$/ })
    .click()
  const calendar = page.locator(`[data-slot="calendar"]`).last()

  await page
    .locator(`[data-slot="calendar"] [data-day="${dataDay}"]`)
    .last()
    .click()
  await scope.getByPlaceholder(`Issue title`).click()
  await expect(calendar).toBeHidden()
}

async function attachImage(
  page: Page,
  scope: Locator,
  filename = `draft-image.png`
) {
  const fileChooserPromise = page.waitForEvent(`filechooser`)
  await scope.getByLabel(`Add image`).click()
  const fileChooser = await fileChooserPromise
  await fileChooser.setFiles({
    name: filename,
    mimeType: `image/png`,
    buffer: PNG_BUFFER,
  })
  // EXP-878: uploads are EAGER — on the New issue page they go to the draft's
  // own route and the FINAL attachment URL is what lands in the description.
  // Wait for that, or the next click races an in-flight upload.
  await expect(scope.locator(`img.editor-image`).last()).toHaveAttribute(
    `src`,
    /\/api\/attachments\//,
    { timeout: 20_000 }
  )
}

/** EXP-1170: New issue is a PAGE — a fresh draft id in the URL. */
async function openNewIssuePage(page: Page) {
  await page.getByRole(`button`, { name: `New issue`, exact: true }).click()
  await expect(page).toHaveURL(/\/t\/[^/]+\/drafts\/[0-9a-f-]{36}/)
  const draftPage = page.getByTestId(`issue-draft-page`)
  await expect(draftPage).toBeVisible()
  return draftPage
}

/**
 * EXP-878/1170: Create LANDS on the issue it just filed (replacing the draft
 * page in history). Every board-list assertion that follows a create has to
 * step back explicitly — asserting the landing on the way past is the
 * cheapest coverage of it.
 */
async function backToBoardAfterCreate(page: Page, app: AppFixture) {
  await expect(page).toHaveURL(
    new RegExp(`/t/[^/]+/boards/${app.boardSlug}/issues/`)
  )
  await page.goto(page.url().replace(/\/issues\/.*$/, ``))
  await expect(page.locator(`[data-testid^="issue-row-"]`).first()).toBeVisible()
}

/** The issue detail's body (title, tray, description) on md+. */
function issueDetail(page: Page) {
  return page.locator(`[data-detail-scroll]`)
}

test(`creates an issue on the New issue page and edits it on the detail`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createBoard(page, app)

  await expect(page).toHaveURL(
    new RegExp(`/t/[^/]+/boards/${app.boardSlug}/?$`)
  )

  const draftPage = await openNewIssuePage(page)
  // The header names the page; there is no identifier yet.
  await expect(draftPage.getByRole(`button`, { name: `Create` })).toBeDisabled()

  await draftPage.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  await replaceIssueDescription(page, draftPage, app.issueDescription)

  await draftPage.getByRole(`button`, { name: `Label` }).click()
  await page.getByText(`Create label`).click()
  await page.getByPlaceholder(`Label name`).fill(app.labelName)
  await page.getByLabel(`Select label color ${app.labelColor}`).click()
  await page.getByRole(`button`, { name: `Create label` }).click()
  await page.keyboard.press(`Escape`)

  await draftPage.getByRole(`button`, { name: `Assignee` }).click()
  await page.getByText(app.owner.name, { exact: true }).click()

  await draftPage.getByRole(`button`, { name: `No priority` }).click()
  await page.getByRole(`menuitem`, { name: `High` }).click()

  await selectDueDate(page, draftPage, app.dueDate.dataDay)

  await draftPage.getByRole(`button`, { name: `Create` }).click()
  await backToBoardAfterCreate(page, app)

  const createdRow = page
    .locator(`[data-testid^="issue-row-"]`)
    .filter({ hasText: app.issueTitle })

  await expect(createdRow).toHaveCount(1)
  await expect(createdRow).toBeVisible()
  await expect(createdRow).toContainText(app.labelName)
  await expect(createdRow).toContainText(app.dueDate.text)
  await expect(createdRow).toContainText(app.owner.initials)

  const identifier = (
    await createdRow.locator(`span.font-mono`).textContent()
  )?.trim()

  if (!identifier) {
    throw new Error(
      `Expected a generated issue identifier after issue creation.`
    )
  }

  const row = page.locator(`[data-testid="issue-row-${identifier}"]`)
  await row.click()

  // Editing happens on the issue detail: title + description save on blur,
  // the property chips at once.
  const detail = issueDetail(page)
  await expect(detail.getByPlaceholder(`Issue title`)).toHaveValue(
    app.issueTitle
  )
  await detail.getByPlaceholder(`Issue title`).fill(app.updatedIssueTitle)
  await replaceIssueDescription(page, detail, app.updatedIssueDescription)
  await detail.getByPlaceholder(`Issue title`).click()

  await detail.getByRole(`button`, { name: `Backlog` }).click()
  await page.getByRole(`menuitem`, { name: `In Progress` }).click()

  await detail.getByRole(`button`, { name: `High` }).click()
  await page.getByRole(`menuitem`, { name: `Urgent` }).click()

  await page.reload()
  await expect(detail.getByPlaceholder(`Issue title`)).toHaveValue(
    app.updatedIssueTitle
  )
  await expect(detail.getByLabel(`Issue description`)).toContainText(
    app.updatedIssueDescription
  )
  await expect(detail.getByRole(`button`, { name: `In Progress` })).toBeVisible()
  await expect(detail.getByRole(`button`, { name: `Urgent` })).toBeVisible()
})

test(`uploads images into the draft and carries them onto the created issue`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createBoard(page, app)

  const draftPage = await openNewIssuePage(page)
  await draftPage.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  await replaceIssueDescription(page, draftPage, app.issueDescription)
  await attachImage(page, draftPage)

  // EXP-878: the page uploads eagerly into its DRAFT, so the image is already
  // a real attachment before the issue exists — no blob: URL, and nothing
  // left to upload after the create. Images render inline only.
  const draftImage = draftPage.locator(`img.editor-image`)
  await expect(draftImage).toHaveCount(1)
  await expect(draftImage).toHaveAttribute(`src`, /\/api\/attachments\//)
  await expect(draftPage.getByTestId(`issue-files-section`)).toHaveCount(0)

  await draftPage.getByRole(`button`, { name: `Create` }).click()
  await expect(page).toHaveURL(
    new RegExp(`/t/[^/]+/boards/${app.boardSlug}/issues/`)
  )

  const detail = issueDetail(page)
  await expect(detail.getByLabel(`Issue description`)).toContainText(
    app.issueDescription
  )
  await expect(detail.locator(`img.editor-image`)).toHaveCount(1)
  await expect(detail.locator(`img.editor-image`)).toHaveAttribute(
    `src`,
    /\/api\/attachments\//
  )

  // The inline hover control removes it from the issue.
  const imageNode = detail.locator(`.editor-image-node`).first()
  await imageNode.hover()
  await imageNode
    .getByRole(`button`, { name: `Image options for draft-image.png` })
    .click()
  await page.getByRole(`menuitem`, { name: `Delete` }).click()
  await expect(detail.locator(`img.editor-image`)).toHaveCount(0)

  await page.reload()
  await expect(detail.getByLabel(`Issue description`)).toContainText(
    app.issueDescription
  )
  await expect(detail.locator(`img.editor-image`)).toHaveCount(0)
})

test(`keeps a draft on Back, reopens it from Drafts, and discards it`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createBoard(page, app)

  const draftPage = await openNewIssuePage(page)
  const draftUrl = page.url()
  // The open draft is the page itself, never a sidebar entry.
  await expect(page.getByRole(`link`, { name: `Drafts` })).toHaveCount(0)

  await draftPage.getByPlaceholder(`Issue title`).fill(app.issueTitle)
  // Autosave: one coalesced write shortly after the last keystroke.
  await page.waitForTimeout(1_500)

  // EXP-1212: leaving a draft with content is held and asks first.
  await page.goBack()
  const leaveDialog = page.getByTestId(`issue-draft-leave-dialog`)
  await expect(leaveDialog).toBeVisible()
  await leaveDialog.getByRole(`button`, { name: `Save draft` }).click()
  const draftsEntry = page.getByRole(`link`, { name: `Drafts` })
  await expect(draftsEntry).toBeVisible()
  await draftsEntry.click()

  const list = page.getByTestId(`drafts-list`)
  await expect(list).toContainText(app.issueTitle)
  await list.getByText(app.issueTitle).click()

  // The row reopens the SAME draft id.
  const draftId = new URL(draftUrl).pathname.split(`/`).pop()
  await expect(page).toHaveURL(new RegExp(`/drafts/${draftId}`))
  const reopened = page.getByTestId(`issue-draft-page`)
  await expect(reopened.getByPlaceholder(`Issue title`)).toHaveValue(
    app.issueTitle
  )

  // EXP-1212: the × on a draft with content confirms first.
  await reopened.getByTestId(`issue-draft-discard`).click()
  const discardConfirm = page.getByTestId(`issue-draft-discard-confirm`)
  await expect(discardConfirm).toContainText(`Discard this draft and its files?`)
  await discardConfirm
    .getByRole(`button`, { name: `Discard`, exact: true })
    .click()

  // Back to the Drafts list it came from, which has emptied out.
  await expect(page).toHaveURL(/\/drafts\/?$/)
  await expect(page.getByTestId(`drafts-list`)).toHaveCount(0)
  await expect(page.getByRole(`link`, { name: `Drafts` })).toHaveCount(0)
})
