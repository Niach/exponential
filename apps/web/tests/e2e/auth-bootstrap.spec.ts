import {
  createTeamThroughOnboarding,
  loginUser,
  logoutUser,
  registerUser,
} from "./helpers/auth"
import { expect, test } from "./fixtures"

test(`registers, creates a team through onboarding, and signs back in`, async ({
  app,
  page,
}) => {
  await page.goto(`/`)
  await expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/)

  // Fresh accounts get no team (EXP-188): registration lands on the
  // onboarding create-or-join choice.
  await registerUser(page, app.owner)
  const teamSlug = await createTeamThroughOnboarding(
    page,
    `${app.namespace} team`,
    app.boardName
  )

  await expect(
    page.getByRole(`link`, { name: app.boardName })
  ).toBeVisible()

  await logoutUser(page)
  await loginUser(page, app.owner)

  // #EXP-2027: md+ lands on the team-less Home (no active team).
  await expect(page).toHaveURL(new RegExp(`/(home|t/${teamSlug})(/|$)`))
})
