import { expect, type Page } from "@playwright/test"
import type { TestUser } from "../fixtures"

interface AuthOptions {
  expectedPath?: RegExp
  fromCurrentPage?: boolean
  redirectPath?: string
}

function buildAuthPath(pathname: string, redirectPath?: string) {
  if (!redirectPath) {
    return pathname
  }

  const search = new URLSearchParams({ redirect: redirectPath })
  return `${pathname}?${search.toString()}`
}

// Signup and login are ONE merged /auth/login page (EXP-188): registration
// flips the card into create-account mode via the footer toggle. Fresh
// accounts have no team, so registration lands on /onboarding by default.
export async function registerUser(
  page: Page,
  user: TestUser,
  options: AuthOptions = {}
) {
  if (!options.fromCurrentPage) {
    await page.goto(buildAuthPath(`/auth/login`, options.redirectPath))
  }

  await expect(
    page
      .locator(`[data-slot="card-title"]`)
      .filter({ hasText: `Continue to Exponential` })
  ).toBeVisible()
  // EXP-857: the method list comes first; the password form (and its
  // create-account toggle) lives behind "Continue with email" on instances
  // without a mail transport, which the e2e stack is.
  await page.getByRole(`button`, { name: `Continue with email` }).click()
  await page.getByRole(`button`, { name: `Create one` }).click()
  await expect(
    page
      .locator(`[data-slot="card-title"]`)
      .filter({ hasText: `Create an account` })
  ).toBeVisible()

  await page.getByLabel(`Name`).fill(user.name)
  await page.getByLabel(`Email`).fill(user.email)
  // exact: the "Show password" visibility toggle also substring-matches.
  await page.getByLabel(`Password`, { exact: true }).fill(user.password)

  await Promise.all([
    expect(page).toHaveURL(options.expectedPath ?? /\/onboarding\/?$/),
    page.getByRole(`button`, { name: `Create account` }).click(),
  ])
}

export async function loginUser(
  page: Page,
  user: TestUser,
  options: AuthOptions = {}
) {
  if (!options.fromCurrentPage) {
    await page.goto(buildAuthPath(`/auth/login`, options.redirectPath))
  }

  await expect(
    page
      .locator(`[data-slot="card-title"]`)
      .filter({ hasText: `Continue to Exponential` })
  ).toBeVisible()
  await page.getByRole(`button`, { name: `Continue with email` }).click()

  await page.getByLabel(`Email`).fill(user.email)
  await page.getByLabel(`Password`, { exact: true }).fill(user.password)

  await Promise.all([
    // Existing users land somewhere under /t/ (team root or their
    // last-visited board).
    expect(page).toHaveURL(options.expectedPath ?? /\/t\/[^/]+/),
    page.getByRole(`button`, { name: `Continue`, exact: true }).click(),
  ])
}

// Drives the post-signup onboarding wizard (EXP-188, reordered in EXP-725):
// create-or-join choice → team name → first board → invite (skipped) →
// devices (skipped), ending on the new team's page.
export async function createTeamThroughOnboarding(
  page: Page,
  teamName: string,
  boardName: string
) {
  await expect(page).toHaveURL(/\/onboarding\/?$/)
  await page.getByRole(`button`, { name: /Create a team/ }).click()

  await page.getByLabel(`Team name`).fill(teamName)
  await page.getByRole(`button`, { name: `Create team`, exact: true }).click()

  // The board step (EXP-725 order: team, board, invite, devices). It mounts
  // while the new team is still syncing in, and a value typed before that
  // settles is lost — so type until the field holds it.
  const name = page.getByRole(`textbox`, { name: `Name` })
  await expect(name).toBeVisible()
  await expect
    .poll(async () => {
      await name.click()
      await name.fill(boardName)
      await page.waitForTimeout(400)
      return name.inputValue()
    })
    .toBe(boardName)
  await page.getByRole(`button`, { name: `Create board` }).click()

  // The invite and devices steps, skipped; each arrives once the row it
  // waits on has synced, which can take a while on a busy dev stack.
  const skip = page.getByRole(`button`, { name: `Skip for now`, exact: true })
  const onTeam = () => /\/t\/[^/]+/.test(new URL(page.url()).pathname)
  for (let step = 0; step < 4; step += 1) {
    await expect
      .poll(async () => onTeam() || (await skip.isVisible()), { timeout: 60_000 })
      .toBe(true)
    if (onTeam()) break
    await skip.click()
    await page.waitForTimeout(300)
  }
  await expect(page).toHaveURL(/\/t\/[^/]+/, { timeout: 30_000 })
}

export async function logoutUser(page: Page) {
  // first(): the sidebar and the mobile topbar both render a user menu.
  await page.getByLabel(`User menu`).first().click()

  await Promise.all([
    expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/),
    page.getByRole(`menuitem`, { name: `Sign out` }).click(),
  ])
}
