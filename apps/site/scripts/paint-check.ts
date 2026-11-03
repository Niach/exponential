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

// 5. /motion/ and the Motion guide: every player paints (the first two,
//    the eight gallery tiles, the three showcase steps, the like button and
//    the level bar), under the production CSP, in both themes.
async function canvasesPaint(page: import("playwright").Page, label: string, expected: number) {
  // Islands hydrate on view: walk the page so every one mounts.
  const height = await page.evaluate(() => document.body.scrollHeight)
  for (let y = 0; y < height; y += 400) {
    await page.evaluate((top) => window.scrollTo(0, top), y)
    await page.waitForTimeout(120)
  }
  let painted: number[] = []
  for (let i = 0; i < 60; i++) {
    painted = await page.$$eval(`[data-xui-surface] canvas`, (canvases) =>
      canvases.map((c) => {
        const el = c as HTMLCanvasElement
        // A box laid out at 0 px still holds a 1×1 backing store: count it as empty.
        if (el.clientWidth < 8 || el.clientHeight < 8) return 0
        // A tile scrolled off-screen pauses on its poster, still painted.
        const d = el.width && el.height ? el.getContext(`2d`)?.getImageData(0, 0, el.width, el.height).data : undefined
        let n = 0
        if (d) for (let j = 3; j < d.length; j += 4) if (d[j]! > 0) n++
        return n
      })
    )
    if (painted.length >= expected && painted.every((n) => n > 0)) return
    await page.waitForTimeout(250)
  }
  problems.push(`${label}: Motion players painted ${JSON.stringify(painted)} (expected ${expected}, all > 0)`)
}
for (const theme of [`dark`, `light`] as const) {
  for (const [path, expected] of [[`/motion/`, 15], [`/docs/libraries/motion/guides/animate/`, 12]] as const) {
    const { ctx, page } = await open(path, theme)
    await canvasesPaint(page, `${path} ${theme}`, expected)
    await ctx.close()
  }
}

// 6. The eUI playground: prerendered editor, hydrated stage in the site's
//    mode, an example switch reaches the preview.
for (const theme of [`dark`, `light`] as const) {
  const path = `/docs/libraries/eui/tools/playground/`
  const { ctx, page } = await open(path, theme)
  const ready = await page
    .locator(`.eui-playground[data-ready="true"] .eui-stage .xui-surface`)
    .waitFor({ timeout: 20_000 })
    .then(() => true, () => false)
  if (!ready) problems.push(`${path} ${theme}: the playground did not hydrate`)
  else {
    const mode = await page.$eval(`.eui-stage [data-xui-mode]`, (e) => e.getAttribute(`data-xui-mode`))
    if (mode !== theme) problems.push(`${path} ${theme}: stage in mode ${mode}`)
    await page.selectOption(`select[aria-label="Starter example"]`, `form`)
    const switched = await page
      .locator(`.eui-stage`)
      .getByText(`Join the beta`)
      .first()
      .waitFor({ timeout: 5_000 })
      .then(() => true, () => false)
    if (!switched) problems.push(`${path} ${theme}: the example switch did not reach the preview`)
  }
  await ctx.close()
}

// 7. Pictures of the key pages (CI uploads them; mint captures nothing).
const PAGES = [`/`, `/ui/`, `/motion/`, `/apps/`, `/talk/`, `/net/`, `/pilot/`, `/mcp/`, `/agent/`, `/core/`, `/docs/`, `/docs/using/sessions/`, `/docs/eapps/`, `/docs/build/mcp/`, `/docs/libraries/motion/`, `/docs/libraries/motion/guides/animate/`, `/docs/libraries/motion/reference/`, `/docs/libraries/eui/`, `/docs/libraries/eui/guides/react/`, `/docs/libraries/eui/reference/components/`, `/docs/libraries/eui/reference/components/button/`, `/docs/libraries/eui/reference/themes/`, `/docs/libraries/eui/tools/playground/`, `/docs/libraries/eui/tools/theme-builder/`, `/docs/libraries/etalk/`, `/pricing/`, `/download/`, `/contact/`, `/imprint/`, `/blog/`, `/blog/exponentialos/`, `/blog/session-as-interface/`]
if (shots) {
  for (const path of PAGES) {
    for (const theme of [`dark`, `light`] as const) {
      for (const [form, viewport] of [[`web`, { width: 1280, height: 900 }], [`web-mobile`, { width: 390, height: 844 }]] as const) {
        const ctx = await browser.newContext({ viewport, colorScheme: theme, deviceScaleFactor: form === `web` ? 1 : 2 })
        await ctx.addInitScript((t) => localStorage.setItem(`starlight-theme`, t), theme)
        const page = await ctx.newPage()
        const res = await page.goto(`${base}${path}`, { waitUntil: `networkidle` })
        if (res?.status() === 200) {
          // Walk the page once so lazy images load and client:visible
          // islands (Motion) hydrate before the full-page capture.
          await page.evaluate(async () => {
            for (let y = 0; y < document.body.scrollHeight; y += 600) {
              window.scrollTo(0, y)
              await new Promise((r) => setTimeout(r, 120))
            }
            window.scrollTo(0, 0)
          })
          await page.waitForLoadState(`networkidle`)
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
