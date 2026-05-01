import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

function getWorkspaceSlug(currentUrl: string) {
  const [, , workspaceSlug] = new URL(currentUrl).pathname.split(`/`)
  return workspaceSlug
}

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

test(`creates a project and routes the workspace to its board`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)

  const workspaceEntrySlug = getWorkspaceSlug(page.url())
  await createProject(page, app)

  await expect(page).toHaveURL(
    new RegExp(`/w/[^/]+/projects/${app.projectSlug}/?$`)
  )
  const workspaceSlug = getWorkspaceSlug(page.url())
  await expect(page.getByRole(`heading`, { name: `Issues` })).toBeVisible()
  await expect(page.getByRole(`button`, { name: `New Issue` })).toBeVisible()

  await page.goto(`/w/${workspaceEntrySlug}`)
  await expect(page).toHaveURL(
    new RegExp(`/w/${workspaceSlug}/projects/${app.projectSlug}/?$`)
  )
})
