import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

async function createBoard(page: Page, app: AppFixture) {
  // #EXP-2027: the team menu's New › Board opens the New board PAGE.
  await page.getByTestId(`sidebar-team-row`).first().hover()
  await page.getByTestId(`sidebar-team-menu-button`).first().click()
  await page.getByRole(`menuitem`, { name: `New`, exact: true }).hover()
  await page.getByRole(`menuitem`, { name: `Board`, exact: true }).click()

  await expect(page.getByRole(`heading`, { name: `New board` })).toBeVisible()
  await page.getByLabel(`Name`).fill(app.boardName)
  await expect(page.getByLabel(`Prefix`)).toHaveValue(app.boardPrefix)
  await page.getByRole(`button`, { name: `Create board` }).click()
  await expect(page).toHaveURL(/\/t\/[^/]+\/boards\//)
}

test(`creates a board and routes the team to its board`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)

  await createBoard(page, app)

  await expect(page).toHaveURL(
    new RegExp(`/t/[^/]+/boards/${app.boardSlug}/?$`)
  )
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()
  await expect(page.getByRole(`button`, { name: `New Issue` })).toBeVisible()
})
