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
    page.locator(`[data-slot="card-title"]`).filter({ hasText: `Sign in` })
  ).toBeVisible()
  await page.getByRole(`button`, { name: `Create one` }).click()
  await expect(
    page
      .locator(`[data-slot="card-title"]`)
      .filter({ hasText: `Create an account` })
  ).toBeVisible()

  await page.getByLabel(`Name`).fill(user.name)
  await page.getByLabel(`Email`).fill(user.email)
  await page.getByLabel(`Password`).fill(user.password)

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
    page.locator(`[data-slot="card-title"]`).filter({ hasText: `Sign in` })
  ).toBeVisible()

  await page.getByLabel(`Email`).fill(user.email)
  await page.getByLabel(`Password`).fill(user.password)

  await Promise.all([
    // Existing users land somewhere under /t/ (team root or their
    // last-visited board).
    expect(page).toHaveURL(options.expectedPath ?? /\/t\/[^/]+/),
    page.getByRole(`button`, { name: `Sign in`, exact: true }).click(),
  ])
}

// Drives the post-signup onboarding wizard (EXP-188): create-or-join choice
// → team name → first board, ending on the new team's page.
export async function createTeamThroughOnboarding(
  page: Page,
  teamName: string,
  boardName: string
) {
  await expect(page).toHaveURL(/\/onboarding\/?$/)
  await page.getByRole(`button`, { name: /Create a team/ }).click()

  await page.getByLabel(`Team name`).fill(teamName)
  await page.getByRole(`button`, { name: `Create team`, exact: true }).click()

  await expect(page.getByLabel(`Board name`)).toBeVisible()
  await page.getByLabel(`Board name`).fill(boardName)
  await Promise.all([
    expect(page).toHaveURL(/\/t\/[^/]+/),
    page.getByRole(`button`, { name: `Create board` }).click(),
  ])
}

export async function logoutUser(page: Page) {
  await page.getByLabel(`User menu`).click()

  await Promise.all([
    expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/),
    page.getByRole(`menuitem`, { name: `Sign out` }).click(),
  ])
}
