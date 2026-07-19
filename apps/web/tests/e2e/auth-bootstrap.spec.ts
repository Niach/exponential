import {
  createTeamThroughOnboarding,
  loginUser,
  logoutUser,
  registerUser,
} from "./helpers/auth"
import { expect, test } from "./fixtures"

function getTeamSlug(currentUrl: string) {
  const [, , teamSlug] = new URL(currentUrl).pathname.split(`/`)
  return teamSlug
}

test(`registers, creates a team through onboarding, and signs back in`, async ({
  app,
  page,
}) => {
  await page.goto(`/`)
  await expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/)

  // Fresh accounts get no team (EXP-188): registration lands on the
  // onboarding create-or-join choice.
  await registerUser(page, app.owner)
  await createTeamThroughOnboarding(
    page,
    `${app.namespace} team`,
    app.boardName
  )

  const teamSlug = getTeamSlug(page.url())
  await expect(
    page.getByRole(`link`, { name: app.boardName })
  ).toBeVisible()

  await logoutUser(page)
  await loginUser(page, app.owner)

  await expect(page).toHaveURL(new RegExp(`/t/${teamSlug}(/|$)`))
})
