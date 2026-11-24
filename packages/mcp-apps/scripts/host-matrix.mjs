// #NEW-24 — the verification matrix: every MCP Apps view × {dark, light, no
// host context, expired lease}, drawn by real MCP Apps hosts in headless
// Chromium against the connector harness; one PNG per cell + matrix.json.
//
//   bun run build                                        # the four documents
//   PORT=3931 bun scripts/connector-harness.ts           # the server
//   PORT=3933 SERVER=http://localhost:3931/mcp bun scripts/lease-host.ts
//   # the reference host: modelcontextprotocol/ext-apps examples/basic-host,
//   # built, SERVERS='["http://localhost:3931/mcp"]', on :8931 (sandbox :8932)
//   node scripts/host-matrix.mjs <out dir>               # Node 24, repo Playwright
//
// dark + light  = the reference host (it passes `theme` and the style
//                 variables; the theme follows the browser's colour scheme)
// bare, expired = scripts/lease-host.ts (a host with no context; a host whose
//                 lease on the view ran out), on a LIGHT page so a view that
//                 stays dark is visible as the fallback
import { mkdirSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"

const require = createRequire(new URL(`../../../apps/web/`, import.meta.url))
const { chromium } = require(`playwright`)
const out = process.argv[2] ?? `host-matrix`
const BASIC = process.env.BASIC_HOST ?? `http://localhost:8931`
const LEASE = process.env.LEASE_HOST ?? `http://localhost:3933`
mkdirSync(out, { recursive: true })

const CARD = { title: `Tests`, surface: { root: { id: `s`, component: `Stack`, children: [{ id: `r`, component: `Row`, props: { title: `Unit tests`, subtitle: `97 passed` }, slots: { trailing: { id: `g`, component: `Badge`, props: { text: `Green` } } } }, { id: `m`, component: `Markdown`, props: { text: `Ready to **ship**; see \`apps.ts\`.` } }, { id: `b`, component: `Button`, props: { label: `Ship` }, on: { press: { event: { name: `reply`, context: { text: `Ship` } } } } }] } } }
const VIEWS = [
  { id: `issues`, tool: `exponential_issues`, args: { op: `list`, limit: 8 }, screen: `issues` },
  { id: `issue`, tool: `exponential_issues`, args: { op: `get`, id: `EXP-1800` }, screen: `issue` },
  { id: `inbox`, tool: `exponential_notifications`, args: { op: `list` }, screen: `inbox` },
  { id: `devices`, tool: `exponential_devices`, args: { op: `list` }, screen: `devices` },
  { id: `genui-card`, tool: `exponential_show`, args: CARD, screen: `card` },
  { id: `genui-app`, tool: `exponential_apps`, args: { op: `get`, slug: `standup` }, screen: `app` },
]

const browser = await chromium.launch()
const rows = []
const settle = (page) => page.waitForTimeout(Number(process.env.SETTLE ?? 1500))
const screenOf = (root) => root.getAttribute(`data-screen`)
async function until(root, want, ms = 30_000) {
  const end = Date.now() + ms
  let screen = await screenOf(root)
  while (screen !== want && Date.now() < end) {
    await new Promise((resolve) => setTimeout(resolve, 200))
    screen = await screenOf(root)
  }
  return screen
}
async function facts(frame, root) {
  return {
    screen: await screenOf(root),
    expired: (await root.getAttribute(`data-expired`)) === `true`,
    dark: await frame.locator(`html`).evaluate((el) => el.classList.contains(`dark`)),
    ground: await frame.locator(`html`).evaluate((el) => getComputedStyle(el).getPropertyValue(`--background`).trim()),
    hostVariable: await frame.locator(`html`).evaluate((el) => el.style.getPropertyValue(`--color-background-primary`)),
    line: await frame.locator(`[data-testid="view-expired"]`).count(),
    toasts: await frame.locator(`[data-sonner-toast]`).count(),
  }
}

for (const view of VIEWS) {
  // The reference host, both themes.
  for (const theme of [`dark`, `light`]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2, colorScheme: theme })
    const errors = []
    page.on(`pageerror`, (error) => errors.push(error.message))
    await page.goto(`${BASIC}/?theme=hide`)
    await page.waitForFunction(() => document.querySelectorAll(`select`)[1]?.options.length > 0)
    await page.locator(`select`).nth(1).selectOption(view.tool)
    await page.locator(`textarea`).fill(JSON.stringify(view.args))
    await page.getByRole(`button`, { name: `Call Tool` }).click()
    const frame = page.frameLocator(`iframe`).first().frameLocator(`iframe`).first()
    const root = frame.locator(`[data-screen]`)
    await root.waitFor({ state: `attached`, timeout: 60_000 })
    await until(root, view.screen)
    await settle(page)
    const file = `${view.id}-${theme}.png`
    await page.screenshot({ path: `${out}/${file}`, fullPage: true })
    rows.push({ view: view.id, state: theme, host: `ext-apps basic-host`, file, ...(await facts(frame, root)), errors })
    await page.close()
  }
  // The stand-in host: no context, then an expired lease (the call's last
  // screen, kept by a live mount of the same call in the same page).
  for (const state of [`bare`, `expired`]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2, colorScheme: `light` })
    const errors = []
    page.on(`pageerror`, (error) => errors.push(error.message))
    await page.goto(`${LEASE}/?tool=${view.tool}&args=${encodeURIComponent(JSON.stringify(view.args))}&mode=${state === `bare` ? `bare` : `rebuild`}&theme=light`)
    await page.waitForFunction(() => window.hostReady, null, { timeout: 60_000 })
    const frame = page.frameLocator(`iframe`)
    let root = frame.locator(`[data-screen]`)
    await root.waitFor({ state: `attached`, timeout: 60_000 })
    await until(root, view.screen)
    if (state === `expired`) {
      await settle(page)
      await page.evaluate(() => window.rebuild())
      root = frame.locator(`[data-screen]`)
      await root.waitFor({ state: `attached`, timeout: 60_000 })
      await frame.locator(`[data-testid="view-expired"]`).waitFor({ timeout: 30_000 }).catch(() => errors.push(`no expired line`))
    }
    await settle(page)
    const file = `${view.id}-${state}.png`
    await page.screenshot({ path: `${out}/${file}`, fullPage: true })
    const log = await page.evaluate(() => window.hostLog)
    rows.push({ view: view.id, state, host: `lease-host`, file, ...(await facts(frame, root)), toolCalls: log.filter((method) => method === `tools/call`).length, errors })
    await page.close()
  }
}
await browser.close()
writeFileSync(`${out}/matrix.json`, `${JSON.stringify(rows, null, 2)}\n`)
for (const row of rows) console.log(`${row.view.padEnd(11)} ${row.state.padEnd(8)} screen=${row.screen} dark=${row.dark} expired=${row.expired} line=${row.line} toasts=${row.toasts}${row.errors.length ? ` ERRORS ${row.errors.join(` | `)}` : ``}`)
