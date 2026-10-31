import { chromium } from "playwright"
const base = "http://localhost:5199"
const [w, h, out, path] = [Number(process.argv[2]), Number(process.argv[3]), process.argv[4], process.argv[5]]
const browser = await chromium.launch({ executablePath: "/home/niach/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome" })
const ctx = await browser.newContext({ viewport: { width: w, height: h }, deviceScaleFactor: 2, colorScheme: "dark" })
const page = await ctx.newPage(); page.on("console", m => { if (m.type()==="error") console.log("CONSOLE", m.text().slice(0,300)) }); page.on("pageerror", e => console.log("PAGEERR", String(e).slice(0,300)))
const r = await page.request.post(`${base}/api/auth/sign-in/email`, { data: { email: "danny@shots.test", password: "Password123!x" }, headers: { origin: base } })
if (!r.ok()) throw new Error(await r.text())
await page.goto(`${base}${path}`, { waitUntil: "commit", timeout: 120000 }); await page.waitForTimeout(20000); await page.screenshot({ path: out + ".dbg.png" })
await page.screenshot({ path: out + ".dbg.png" }).catch(()=>{}); await page.waitForSelector(process.argv[6] ?? "[data-testid=apps-consent] [data-app]", { timeout: 60000 })
await page.waitForTimeout(800); if (process.argv[7]) await page.locator(process.argv[7]).scrollIntoViewIfNeeded()
await page.screenshot({ path: out })
await browser.close()
