import { chromium } from "playwright"
const base = `https://localhost:3088`
const [,, mode, out] = process.argv
const browser = await chromium.launch({ timeout: 600000 })
const ctx = await browser.newContext(mode === `mobile`
  ? { ignoreHTTPSErrors: true, viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true, colorScheme: `dark` }
  : { ignoreHTTPSErrors: true, viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: `dark` })
const page = await ctx.newPage()
await page.goto(`${base}/auth/login`, { timeout: 120000 })
const r = await page.evaluate(async () => (await fetch(`/api/auth/sign-in/email`, { method: `POST`, headers: { "content-type": `application/json` }, body: JSON.stringify({ email: `demo@exponential.at`, password: `screenshots-demo` }) })).status)
console.log(`login`, r)
await page.goto(`${base}/t/acme/app-editor/ops-desk?tab=settings`, { timeout: 180000 })
await page.waitForSelector(`[data-testid="app-settings-agent"]`, { timeout: 180000 })
await page.waitForTimeout(3000)
await page.screenshot({ path: out })
await browser.close()
