import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test } from "./fixtures"

function getWorkspaceSlug(currentUrl: string) {
  const [, , workspaceSlug] = new URL(currentUrl).pathname.split(`/`)
  return workspaceSlug
}

async function openWorkspaceSettings(page: Page) {
  await page.getByLabel(`Workspace switcher`).click()
  await page.getByRole(`menuitem`, { name: `Workspace settings` }).click()
  await expect(
    page.getByRole(`heading`, { name: `Workspace Settings` })
  ).toBeVisible()
}

test(`generates an invite and accepts it with a second user`, async ({
  app,
  browser,
  page,
}) => {
  await registerUser(page, app.owner)

  await openWorkspaceSettings(page)
  const workspaceSlug = getWorkspaceSlug(page.url())

  await page.getByRole(`button`, { name: `Generate invite link` }).click()

  const inviteInput = page.locator(`[data-testid="invite-url-input"]`)
  await expect(inviteInput).toBeVisible()

  const inviteUrl = await inviteInput.inputValue()
  await expect(inviteUrl).toMatch(/\/invite\//)

  const memberContext = await browser.newContext({
    baseURL: `https://localhost:3000`,
    ignoreHTTPSErrors: true,
    locale: `en-US`,
    timezoneId: `Europe/Berlin`,
    viewport: {
      width: 1440,
      height: 960,
    },
  })

  try {
    const memberPage = await memberContext.newPage()
    await memberPage.goto(inviteUrl)

    await expect(
      memberPage
        .locator(`[data-slot="card-title"]`)
        .filter({ hasText: `Workspace Invite` })
    ).toBeVisible()
    await memberPage.getByRole(`link`, { name: `Create account` }).click()

    const invitePath = new URL(inviteUrl).pathname
    await registerUser(memberPage, app.member, {
      expectedPath: new RegExp(`${invitePath}$`),
      fromCurrentPage: true,
    })

    await expect(
      memberPage.getByRole(`button`, { name: `Accept Invite` })
    ).toBeVisible()
    await memberPage.getByRole(`button`, { name: `Accept Invite` }).click()

    await expect(memberPage).toHaveURL(new RegExp(`/w/${workspaceSlug}/?$`))

    await openWorkspaceSettings(memberPage)
    await expect(
      memberPage.getByRole(`button`, { name: `Generate invite link` })
    ).toHaveCount(0)
    await expect(memberPage.getByText(`${app.member.name} (you)`)).toBeVisible()
  } finally {
    await memberContext.close()
  }

  await page.reload()
  await expect(
    page.getByRole(`heading`, { name: `Workspace Settings` })
  ).toBeVisible()
  await expect(page.getByText(`${app.owner.name} (you)`)).toBeVisible()
  await expect(page.getByText(app.member.name, { exact: true })).toBeVisible()
})
