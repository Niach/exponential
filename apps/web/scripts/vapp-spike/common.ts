// VAPP-4 spike (throwaway): shared browser setup for the vapp-spike scripts.
// Run from apps/web: `bun scripts/vapp-spike/<script>.ts` (BASE_URL defaults
// to the Caddy proxy; the built server on :5173 sits behind it).
import { chromium, type Browser, type Page } from "@playwright/test"
import { launchContext, login } from "../lib/capture-web"

export const BASE_URL = process.env.BASE_URL ?? `https://localhost:3000`

export async function openSignedIn(
  browser: Browser,
  opts: { width: number; height: number; mobile?: boolean }
): Promise<Page> {
  const context = await launchContext(browser, {
    viewport: { width: opts.width, height: opts.height },
    deviceScaleFactor: opts.mobile ? 3 : 2,
    isMobile: opts.mobile,
  })
  const page = await context.newPage()
  await login(page, BASE_URL)
  return page
}

export async function openSurface(page: Page, query = ``) {
  await page.goto(`${BASE_URL}/vapp-kitchen-sink${query}`)
  await page.getByTestId(`vapp-surface`).waitFor({ timeout: 30_000 })
  await page.waitForTimeout(600)
}

export const launch = () => chromium.launch()
