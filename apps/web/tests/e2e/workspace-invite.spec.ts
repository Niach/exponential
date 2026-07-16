import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test } from "./fixtures"

function getWorkspaceSlug(currentUrl: string) {
  const [, , workspaceSlug] = new URL(currentUrl).pathname.split(`/`)
  return workspaceSlug
}

// Settings is split into per-section pages (EXP-146) — the member list and
// invite UI live on the Members page. The heading is "Settings" while the
// workspace is still solo and "Team Settings" once it has members.
async function openWorkspaceSettings(page: Page) {
  const workspaceSlug = getWorkspaceSlug(page.url())
  await page.goto(`/t/${workspaceSlug}/settings/members`)
  await expect(
    page.getByRole(`heading`, { name: /^(Team )?Settings$/ })
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

    await expect(memberPage).toHaveURL(new RegExp(`/t/${workspaceSlug}/?$`))

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
    page.getByRole(`heading`, { name: /^(Team )?Settings$/ })
  ).toBeVisible()
  await expect(page.getByText(`${app.owner.name} (you)`)).toBeVisible()
  await expect(page.getByText(app.member.name, { exact: true })).toBeVisible()
})
