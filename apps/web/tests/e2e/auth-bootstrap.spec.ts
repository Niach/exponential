import { loginUser, logoutUser, registerUser } from "./helpers/auth"
import { expect, test } from "./fixtures"

function getTeamSlug(currentUrl: string) {
  const [, , teamSlug] = new URL(currentUrl).pathname.split(`/`)
  return teamSlug
}

test(`registers, bootstraps a team, and signs back in`, async ({
  app,
  page,
}) => {
  await page.goto(`/`)
  await expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/)

  await registerUser(page, app.owner)
  const main = page.getByRole(`main`)
  await expect(main.getByText(`No boards yet`)).toBeVisible()
  await expect(
    main.getByText(`Create a board from the sidebar to get started.`)
  ).toBeVisible()

  const teamSlug = getTeamSlug(page.url())

  await logoutUser(page)
  await loginUser(page, app.owner)

  await expect(page).toHaveURL(new RegExp(`/t/${teamSlug}/?$`))
  await expect(main.getByText(`No boards yet`)).toBeVisible()
})
