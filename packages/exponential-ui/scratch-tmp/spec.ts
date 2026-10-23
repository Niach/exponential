import { chromium } from "playwright"
import { spawn } from "node:child_process"
const ids = process.argv[2].split(",")
const width = Number(process.argv[3] ?? 900)
const out = process.argv[4] ?? "/tmp/spec"
const port = 4950
const proc = spawn("bun", [new URL("../../exponential-ui-react/harness/serve.ts", import.meta.url).pathname], { env: { ...process.env, PORT: String(port) }, stdio: ["ignore", "ignore", "inherit"] })
const base = `http://127.0.0.1:${port}`
for (let i = 0; i < 400; i++) { try { if ((await fetch(`${base}/dist/main.js`)).ok) break } catch {} ; await new Promise((r) => setTimeout(r, 150)) }
const browser = await chromium.launch()
for (const id of ids) {
  const context = await browser.newContext({ viewport: { width, height: 700 }, deviceScaleFactor: 2 })
  const page = await context.newPage()
  page.on("pageerror", (e) => console.log("ERR", e.message))
  await page.goto(`${base}/?view=specimen&id=${id}`)
  await page.waitForTimeout(1200)
  await page.screenshot({ path: `${out}-${id}-${width}.png`, fullPage: true })
  await context.close()
}
await browser.close(); proc.kill()
