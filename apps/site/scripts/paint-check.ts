/* The built site in Chromium under the production CSP: no CSP or console
   errors, the build-time eUI card is there before any JS, the islands
   hydrate, and every Motion tile paints non-empty pixels (the old site's
   tiles stayed blank when the CSP refused WebAssembly).
   `bun run scripts/paint-check.ts [baseUrl] [shotsDir]` (serve dist first). */
import { chromium } from "playwright"
import { mkdirSync } from "node:fs"

const base = process.argv[2] ?? `http://localhost:4321`
// The server may still be starting (CI starts it in the background).
for (let i = 0; ; i++) {
  try {
    if ((await fetch(`${base}/ui/`)).ok) break
  } catch {}
  if (i > 100) throw new Error(`${base} is not answering`)
  await Bun.sleep(200)
}
const shots = process.argv[3]
const browser = await chromium.launch()
const problems: string[] = []

async function open(path: string, theme: `dark` | `light`, js = true) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 }, colorScheme: theme, javaScriptEnabled: js, deviceScaleFactor: 1 })
  await ctx.addInitScript((t) => localStorage.setItem(`starlight-theme`, t), theme)
  const page = await ctx.newPage()
  page.on(`console`, (m) => {
    if (m.type() === `error`) problems.push(`${path} ${theme}: console: ${m.text()}`)
  })
  page.on(`pageerror`, (e) => problems.push(`${path} ${theme}: ${e.message}`))
  page.on(`requestfailed`, (r) => problems.push(`${path} ${theme}: request failed ${r.url()} ${r.failure()?.errorText}`))
  await page.goto(`${base}${path}`, { waitUntil: `networkidle` })
  return { ctx, page }
}

// 1. No JS: the build-time card is real HTML.
{
  const { ctx, page } = await open(`/ui/`, `dark`, false)
  if (!(await page.getByText(`Deploy web 0.20.27`).isVisible())) problems.push(`no-JS: the build-time eUI card is missing`)
  await ctx.close()
}

for (const theme of [`dark`, `light`] as const) {
  const { ctx, page } = await open(`/ui/`, theme)
  // 2. The islands hydrate: typing reaches the data model (the echo Text).
  const input = page.getByLabel(`Board name`)
  await input.scrollIntoViewIfNeeded()
  // Astro drops the island's `ssr` attribute once React has hydrated it.
  await page.locator(`astro-island:not([ssr]) [data-xui-surface="xui-settings"]`).waitFor({ timeout: 20_000 })
  await input.fill(`Mobile`)
  const echoed = await page
    .locator(`[data-xui-surface="xui-settings"]`)
    .getByText(`Mobile`, { exact: true })
    .waitFor({ timeout: 5_000 })
    .then(() => true, () => false)
  if (!echoed) problems.push(`${theme}: the settings island did not hydrate`)
  // 3. Motion paints: every canvas has non-transparent pixels.
  const motion = page.locator(`[data-xui-surface="xui-motion"]`)
  await motion.scrollIntoViewIfNeeded()
  // The WASM engine loads on first view: poll (slow CI runners) up to 15 s.
  let painted: number[] = []
  for (let i = 0; i < 60; i++) {
    painted = await page.$$eval(`[data-xui-surface="xui-motion"] canvas`, (canvases) =>
      canvases.map((c) => {
        const el = c as HTMLCanvasElement
        const d = el.width && el.height ? el.getContext(`2d`)?.getImageData(0, 0, el.width, el.height).data : undefined
        let n = 0
        if (d) for (let j = 3; j < d.length; j += 4) if (d[j]! > 0) n++
        return n
      })
    )
    if (painted.length === 8 && painted.every((n) => n > 0)) break
    await page.waitForTimeout(250)
  }
  if (painted.length !== 8 || painted.some((n) => n === 0)) {
    const states = await page.$$eval(`[data-xui-surface="xui-motion"] [data-state]`, (els) => els.map((e) => e.getAttribute(`data-state`)))
    problems.push(`${theme}: Motion tiles painted ${JSON.stringify(painted)}, states ${JSON.stringify(states)}`)
  }
  // 4. The surfaces follow the site theme.
  const modes = await page.$$eval(`[data-xui-mode]`, (els) => els.map((e) => e.getAttribute(`data-xui-mode`)))
  if (modes.some((m) => m !== theme)) problems.push(`${theme}: surfaces in modes ${modes.join(`,`)}`)
  if (shots) {
    mkdirSync(shots, { recursive: true })
    await page.evaluate(() => window.scrollTo(0, 0))
    await page.screenshot({ path: `${shots}/ui-${theme}.png`, fullPage: true })
    await motion.screenshot({ path: `${shots}/motion-${theme}.png` })
    await page.locator(`[data-xui-surface="xui-settings"]`).screenshot({ path: `${shots}/island-${theme}.png` })
  }
  await ctx.close()
}

// 5. Pictures of the key pages (CI uploads them; mint captures nothing).
const PAGES = [`/`, `/ui/`, `/motion/`, `/apps/`, `/talk/`, `/net/`, `/pilot/`, `/mcp/`, `/agent/`, `/core/`, `/docs/`, `/docs/using/sessions/`, `/docs/eapps/`, `/docs/build/mcp/`, `/docs/libraries/motion/reference/`, `/docs/libraries/etalk/`, `/pricing/`, `/download/`, `/contact/`, `/imprint/`, `/blog/`, `/blog/exponentialos/`, `/blog/session-as-interface/`]
if (shots) {
  for (const path of PAGES) {
    for (const theme of [`dark`, `light`] as const) {
      for (const [form, viewport] of [[`web`, { width: 1280, height: 900 }], [`web-mobile`, { width: 390, height: 844 }]] as const) {
        const ctx = await browser.newContext({ viewport, colorScheme: theme, deviceScaleFactor: form === `web` ? 1 : 2 })
        await ctx.addInitScript((t) => localStorage.setItem(`starlight-theme`, t), theme)
        const page = await ctx.newPage()
        const res = await page.goto(`${base}${path}`, { waitUntil: `networkidle` })
        if (res?.status() === 200) {
          await page.waitForTimeout(1500)
          const name = path === `/` ? `home` : path.replace(/^\/|\/$/g, ``).replace(/\//g, `-`)
          await page.screenshot({ path: `${shots}/${form}-${name}-${theme}.png`, fullPage: form === `web` })
        }
        await ctx.close()
      }
    }
  }
}

await browser.close()
if (problems.length) {
  console.error(problems.join(`\n`))
  process.exit(1)
}
console.log(`paint check: ok`)
