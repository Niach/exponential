import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

function getTeamSlug(currentUrl: string) {
  const [, , teamSlug] = new URL(currentUrl).pathname.split(`/`)
  return teamSlug
}

async function createBoard(page: Page, app: AppFixture) {
  await page.getByLabel(`Create board`).click()

  const dialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Create board` }),
  })

  await expect(dialog).toBeVisible()
  await dialog.getByLabel(`Name`).fill(app.boardName)
  await expect(dialog.getByLabel(`Prefix`)).toHaveValue(app.boardPrefix)

  await dialog.getByRole(`button`, { name: `Create board` }).click()
  await expect(dialog).toBeHidden()
}

test(`creates a board and routes the team to its board`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)

  const teamEntrySlug = getTeamSlug(page.url())
  await createBoard(page, app)

  await expect(page).toHaveURL(
    new RegExp(`/t/[^/]+/boards/${app.boardSlug}/?$`)
  )
  const teamSlug = getTeamSlug(page.url())
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()
  await expect(page.getByRole(`button`, { name: `New Issue` })).toBeVisible()

  await page.goto(`/t/${teamEntrySlug}`)
  await expect(page).toHaveURL(
    new RegExp(`/t/${teamSlug}/boards/${app.boardSlug}/?$`)
  )
})
