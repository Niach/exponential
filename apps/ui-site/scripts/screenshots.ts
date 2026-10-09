/* Screenshots of the built site (run `bun run build` first): serves dist/
   the way production does (a path → <path>/index.html) and captures the
   main pages, dark and light, desktop and phone, into $OUT (default
   /tmp/ui-site-shots). `bun run screenshots [--only components]` */
import { existsSync, mkdirSync, statSync } from "node:fs"
import { join, resolve } from "node:path"
import { chromium, type Page } from "playwright"

const DIST = resolve(import.meta.dirname, `../dist`)
const OUT = process.env.OUT ?? `/tmp/ui-site-shots`
const only = process.argv.includes(`--only`) ? process.argv[process.argv.indexOf(`--only`) + 1] : undefined

if (!existsSync(join(DIST, `index.html`))) throw new Error(`dist/ missing: bun run build first`)
mkdirSync(OUT, { recursive: true })

const server = Bun.serve({
  port: 0,
  fetch(req) {
    const url = new URL(req.url)
    let file = join(DIST, decodeURIComponent(url.pathname))
    if (!file.startsWith(DIST)) return new Response(`no`, { status: 403 })
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, `index.html`)
    if (!existsSync(file)) return new Response(`not found`, { status: 404 })
    return new Response(Bun.file(file))
  },
})
const base = `http://localhost:${server.port}`

/** `prerender`: the bundle is blocked, so the shot is the first paint before
 *  hydration (only the inline scheme script runs). */
type Shot = { name: string; path: string; width?: number; height?: number; scheme?: `light` | `dark`; full?: boolean; prerender?: boolean; before?: (page: Page) => Promise<void> }

const SHOTS: Shot[] = [
  { name: `home`, path: `/` },
  { name: `home-light`, path: `/`, scheme: `light` },
  { name: `components`, path: `/components/`, full: true },
  { name: `components-light`, path: `/components/`, scheme: `light` },
  { name: `components-filter-inputs`, path: `/components/?group=input` },
  { name: `components-search`, path: `/components/?q=date` },
  { name: `components-phone`, path: `/components/`, width: 390, height: 844 },
  { name: `components-phone-light`, path: `/components/`, width: 390, height: 844, scheme: `light` },
  { name: `components-light-prerender`, path: `/components/`, scheme: `light`, prerender: true },
  { name: `component-button`, path: `/components/button/`, full: true },
  { name: `component-button-light`, path: `/components/button/`, scheme: `light` },
  { name: `component-button-phone`, path: `/components/button/`, width: 390, height: 844, full: true },
  { name: `component-button-light-prerender`, path: `/components/button/`, scheme: `light`, prerender: true },
  {
    name: `component-button-variants-light`,
    path: `/components/button/`,
    scheme: `light`,
    before: async (page) => {
      await page.locator(`#variants`).scrollIntoViewIfNeeded()
      await page.waitForTimeout(900)
    },
  },
  {
    name: `component-button-rtl-768`,
    path: `/components/button/`,
    before: async (page) => {
      await page.getByRole(`button`, { name: `RTL` }).click()
      await page.getByRole(`button`, { name: `768 px` }).click()
    },
  },
  {
    name: `component-button-react-tab`,
    path: `/components/button/`,
    before: async (page) => {
      await page.getByRole(`tab`, { name: `React` }).click()
      await page.locator(`#panel-react`).scrollIntoViewIfNeeded()
    },
  },
  {
    name: `component-table-1280`,
    path: `/components/table/`,
    before: async (page) => {
      await page.getByRole(`button`, { name: `1280 px` }).click()
    },
  },
  { name: `component-resizable`, path: `/components/resizable/`, full: true },
  { name: `component-dialog`, path: `/components/dialog/`, full: true },
  { name: `component-date-range-picker`, path: `/components/date-range-picker/`, full: true },
  { name: `concepts`, path: `/concepts/` },
  { name: `themes`, path: `/themes/` },
  { name: `theme-builder`, path: `/themes/builder/` },
  { name: `guides`, path: `/guides/` },
  { name: `guide-react`, path: `/guides/react/` },
  { name: `playground`, path: `/playground/` },
  { name: `conformance`, path: `/conformance/` },
]

/** Scroll the page through once so the near-viewport renders mount. */
async function warm(page: Page) {
  const height = await page.evaluate(() => document.body.scrollHeight)
  for (let y = 0; y < height; y += 600) {
    await page.evaluate((top) => window.scrollTo(0, top), y)
    await page.waitForTimeout(120)
  }
  await page.evaluate(() => window.scrollTo(0, 0))
  await page.waitForTimeout(600)
}

const browser = await chromium.launch()
const written: string[] = []
try {
  for (const shot of SHOTS.filter((s) => !only || s.name.includes(only))) {
    const context = await browser.newContext({ viewport: { width: shot.width ?? 1440, height: shot.height ?? 900 }, deviceScaleFactor: 1, colorScheme: shot.scheme ?? `dark` })
    const page = await context.newPage()
    const errors: string[] = []
    page.on(`pageerror`, (e) => errors.push(e.message))
    if (shot.prerender) await page.route(`**/assets/*.js`, (route) => route.abort())
    await page.goto(`${base}${shot.path}`, { waitUntil: `load` })
    await page.waitForTimeout(900)
    if (shot.full) await warm(page)
    if (shot.before) await shot.before(page)
    await page.waitForTimeout(700)
    const file = join(OUT, `${shot.name}.png`)
    await page.screenshot({ path: file, fullPage: shot.full ?? false })
    written.push(file)
    console.log(`${file}${errors.length ? `  (page errors: ${errors.join(` | `)})` : ``}`)
    await context.close()
  }
} finally {
  await browser.close()
  server.stop(true)
}
console.log(`${written.length} screenshots`)
