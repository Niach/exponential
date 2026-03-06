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

export async function registerUser(
  page: Page,
  user: TestUser,
  options: AuthOptions = {}
) {
  if (!options.fromCurrentPage) {
    await page.goto(buildAuthPath(`/auth/register`, options.redirectPath))
  }

  await expect(
    page.locator(`[data-slot="card-title"]`).filter({ hasText: `Create an account` })
  ).toBeVisible()

  await page.getByLabel(`Name`).fill(user.name)
  await page.getByLabel(`Email`).fill(user.email)
  await page.getByLabel(`Password`).fill(user.password)

  await Promise.all([
    expect(page).toHaveURL(options.expectedPath ?? /\/w\/[^/]+\/?$/),
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
    expect(page).toHaveURL(options.expectedPath ?? /\/w\/[^/]+\/?$/),
    page.getByRole(`button`, { name: `Sign in` }).click(),
  ])
}

export async function logoutUser(page: Page) {
  await page.getByLabel(`User menu`).click()

  await Promise.all([
    expect(page).toHaveURL(/\/auth\/login(?:\?.*)?$/),
    page.getByRole(`menuitem`, { name: `Sign out` }).click(),
  ])
}
