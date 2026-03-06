import { loginUser, logoutUser, registerUser } from "./helpers/auth"
import { expect, test } from "./fixtures"

function getWorkspaceSlug(currentUrl: string) {
  const [, , workspaceSlug] = new URL(currentUrl).pathname.split(`/`)
  return workspaceSlug
}

test(`registers, bootstraps a workspace, and signs back in`, async ({
  app,
  page,
}) => {
  await page.goto(`/`)
  await expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/)

  await registerUser(page, app.owner)
  const main = page.getByRole(`main`)
  await expect(main.getByText(`No projects yet`)).toBeVisible()
  await expect(
    main.getByText(`Create a project from the sidebar to get started.`)
  ).toBeVisible()

  const workspaceSlug = getWorkspaceSlug(page.url())

  await logoutUser(page)
  await loginUser(page, app.owner)

  await expect(page).toHaveURL(new RegExp(`/w/${workspaceSlug}/?$`))
  await expect(main.getByText(`No projects yet`)).toBeVisible()
})
