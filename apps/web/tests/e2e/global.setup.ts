import { chromium } from "@playwright/test"
import {
  assertDatabaseReachable,
  assertElectricReachable,
  assertIssueNumberTriggerExists,
} from "./helpers/db"

// A fresh `bun dev` compiles the client on its first page load, which takes
// longer than a test's expect timeout on CI: load the sign-in page once here.
async function warmUp() {
  const browser = await chromium.launch()
  try {
    const page = await browser.newPage({ ignoreHTTPSErrors: true })
    await page.goto(`https://localhost:3000/auth/login`, { timeout: 180_000 })
    await page
      .getByRole(`heading`, { level: 1, name: `Continue to Exponential` })
      .waitFor({ timeout: 180_000 })
  } finally {
    await browser.close()
  }
}

export default async function globalSetup() {
  await assertDatabaseReachable()
  await assertElectricReachable()
  await assertIssueNumberTriggerExists()
  await warmUp()
}
