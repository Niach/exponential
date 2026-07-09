// End-to-end coverage for the embeddable feedback widget: settings UI →
// public cross-origin endpoints → the real widget on the built demo page →
// issue visible in the project board.
//
// Requires the widget artifacts in apps/web/public/widget/v1 (run
// `bun run build:widget` from the repo root before the e2e suite).
import type { Page } from "@playwright/test"
import { registerUser } from "./helpers/auth"
import { expect, test, type AppFixture } from "./fixtures"

const tinyPngBase64 = `iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==`

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
  await dialog.getByRole(`button`, { name: `Create project` }).click()
  await expect(dialog).toBeHidden()
}

async function createWidget(
  page: Page,
  app: AppFixture,
  workspaceSlug: string,
  domains: string
): Promise<string> {
  await page.goto(`/w/${workspaceSlug}/settings`)
  await expect(
    page.getByRole(`heading`, { name: `Workspace Settings` })
  ).toBeVisible()

  await page.getByRole(`button`, { name: `New widget` }).click()
  const dialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `New feedback widget` }),
  })
  await expect(dialog).toBeVisible()
  await dialog.getByLabel(`Name`).fill(`Widget ${app.namespace}`)
  await dialog.getByLabel(`Allowed domains`).fill(domains)
  await dialog.getByRole(`button`, { name: `Create widget` }).click()

  // Creating opens the snippet dialog; the public key is inside the snippet.
  const snippetDialog = page.getByRole(`dialog`).filter({
    has: page.getByRole(`heading`, { name: `Embed snippet` }),
  })
  await expect(snippetDialog).toBeVisible()
  const snippet = await snippetDialog.locator(`pre`).innerText()
  const key = /expw_[A-Za-z0-9]{32}/.exec(snippet)?.[0]
  expect(key, `snippet should contain the public key`).toBeTruthy()
  await page.keyboard.press(`Escape`)
  await expect(snippetDialog).toBeHidden()
  return key!
}

test(`widget endpoints enforce origin rules and create issues with screenshots`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  const workspaceSlug = getWorkspaceSlug(page.url())

  const key = await createWidget(page, app, workspaceSlug, `example.com`)

  // Config endpoint: allowed origin gets CORS echo + form payload.
  const allowedConfig = await page.request.get(
    `/api/widget/config?key=${key}`,
    { headers: { Origin: `https://example.com` } }
  )
  expect(allowedConfig.status()).toBe(200)
  expect(allowedConfig.headers()[`access-control-allow-origin`]).toBe(
    `https://example.com`
  )
  expect(allowedConfig.headers().vary).toContain(`Origin`)
  const configBody = (await allowedConfig.json()) as { enabled: boolean }
  expect(configBody.enabled).toBe(true)

  // Disallowed origin → 403 without ACAO.
  const deniedConfig = await page.request.get(`/api/widget/config?key=${key}`, {
    headers: { Origin: `https://evil.test` },
  })
  expect(deniedConfig.status()).toBe(403)
  expect(
    deniedConfig.headers()[`access-control-allow-origin`]
  ).toBeUndefined()

  // Unknown key → 404.
  const unknownConfig = await page.request.get(
    `/api/widget/config?key=expw_${`x`.repeat(32)}`
  )
  expect(unknownConfig.status()).toBe(404)

  // Submit from an allowed origin with a screenshot.
  const submitted = await page.request.post(`/api/widget/submit`, {
    headers: { Origin: `https://example.com` },
    multipart: {
      key,
      title: `Widget report ${app.namespace}`,
      description: `Steps: clicked the broken button`,
      email: `reporter@example.com`,
      name: `Rita Reporter`,
      customData: JSON.stringify({ plan: `pro` }),
      meta: JSON.stringify({
        url: `https://example.com/checkout`,
        viewportWidth: 1280,
        viewportHeight: 720,
        devicePixelRatio: 2,
      }),
      screenshot: {
        name: `screenshot.png`,
        mimeType: `image/png`,
        buffer: Buffer.from(tinyPngBase64, `base64`),
      },
    },
  })
  expect(submitted.status()).toBe(201)
  const submitBody = (await submitted.json()) as { identifier: string }
  expect(submitBody.identifier).toMatch(new RegExp(`^${app.projectPrefix}-`))

  // Honeypot submissions pretend success but create nothing.
  const honeypot = await page.request.post(`/api/widget/submit`, {
    headers: { Origin: `https://example.com` },
    multipart: {
      key,
      title: `Spam ${app.namespace}`,
      website: `https://spam.example`,
    },
  })
  expect(honeypot.status()).toBe(201)

  // Submit from a disallowed origin → 403.
  const deniedSubmit = await page.request.post(`/api/widget/submit`, {
    headers: { Origin: `https://evil.test` },
    multipart: { key, title: `Nope` },
  })
  expect(deniedSubmit.status()).toBe(403)

  // The real submission shows up in the project; the honeypot one doesn't.
  await page.goto(`/w/${workspaceSlug}/projects/${app.projectSlug}`)
  await expect(
    page.getByText(`Widget report ${app.namespace}`)
  ).toBeVisible()
  await expect(page.getByText(`Spam ${app.namespace}`)).toHaveCount(0)

  // Open the issue: the user's text + screenshot render from the description,
  // while reporter/page/env metadata lives ONLY in the members-only
  // "Reported via widget" card (EXP-42b — no PII in descriptions).
  await page.getByText(`Widget report ${app.namespace}`).click()
  await expect(page.getByText(`Steps: clicked the broken button`)).toBeVisible()
  await expect(page.getByText(`Reported via widget`)).toBeVisible()
  await expect(
    page.getByText(`Rita Reporter <reporter@example.com>`)
  ).toBeVisible()
  await expect(page.getByText(`https://example.com/checkout`)).toBeVisible()
  await expect(
    page.locator(`img[src*="/api/attachments/"]`).first()
  ).toBeVisible()
})

test(`the embedded widget captures, submits, and files an issue`, async ({
  app,
  page,
}) => {
  await registerUser(page, app.owner)
  await createProject(page, app)
  const workspaceSlug = getWorkspaceSlug(page.url())

  // localhost must be allowed for the demo page's origin.
  const key = await createWidget(
    page,
    app,
    workspaceSlug,
    `example.com\nlocalhost`
  )

  await page.goto(`/widget/v1/demo.html?key=${key}`)

  // Loader renders the floating button (shadow DOM — Playwright pierces it).
  const fab = page.locator(`button.exp-fab`)
  await expect(fab).toBeVisible()
  await fab.click()

  // Capture runs before the panel opens; chromium capture is deterministic
  // on the demo page, so a preview image must appear.
  const panel = page.locator(`.exp-panel`)
  await expect(panel).toBeVisible({ timeout: 20_000 })
  const shot = panel.locator(`.exp-shot img`)
  await expect(shot).toBeVisible({ timeout: 20_000 })

  // Annotate: rectangle + arrow drawn over the screenshot, flattened on Done.
  await panel.getByRole(`button`, { name: `Annotate` }).click()
  const annotator = page.locator(`.exp-annotator`)
  await expect(annotator).toBeVisible()
  const canvas = annotator.locator(`canvas`)
  await expect(canvas).toBeVisible()
  const box = (await canvas.boundingBox())!
  await page.mouse.move(box.x + box.width * 0.15, box.y + box.height * 0.15)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.45, {
    steps: 5,
  })
  await page.mouse.up()
  await annotator.getByRole(`button`, { name: `Arrow` }).click()
  await page.mouse.move(box.x + box.width * 0.8, box.y + box.height * 0.8)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.55, {
    steps: 5,
  })
  await page.mouse.up()
  await annotator.getByRole(`button`, { name: `Done` }).click()
  await expect(annotator).toBeHidden()
  await expect(shot).toBeVisible()

  await panel.locator(`#exp-title`).fill(`In-widget report ${app.namespace}`)
  await panel
    .locator(`#exp-description`)
    .fill(`Filed through the embedded widget`)
  await panel.locator(`#exp-email`).fill(`widget-user@example.com`)
  await panel.getByRole(`button`, { name: `Send feedback` }).click()

  await expect(page.getByText(`Thanks for the report!`)).toBeVisible({
    timeout: 15_000,
  })

  // The issue landed in the project the widget points at.
  await page.goto(`/w/${workspaceSlug}/projects/${app.projectSlug}`)
  await expect(
    page.getByText(`In-widget report ${app.namespace}`)
  ).toBeVisible()
})
