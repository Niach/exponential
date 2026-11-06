#!/usr/bin/env bun
// VAPP-269: screenshots of the built runner (`vite build` first) running a
// bundle, in Chromium: the export served as static files with `app/`
// swapped for the given package, at the web (1280×800) and web-mobile
// (390×844) sizes. `--permissions` also opens the Permissions sheet.
//
//   bun run capture -- --package ../../../samples/apps/crm.app.json --core --name crm --out /tmp/runner-shots [--permissions] [--screen <id>]

import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { parseArgs } from "node:util"
import { chromium } from "playwright"

const { values } = parseArgs({ options: { package: { type: `string` }, name: { type: `string` }, out: { type: `string` }, permissions: { type: `boolean` }, core: { type: `boolean` }, screen: { type: `string` }, title: { type: `string` } } })
const pkgPath = resolve(values.package ?? `public/app/package.app.json`)
const name = values.name ?? `app`
const out = resolve(values.out ?? `/tmp/runner-shots`)
mkdirSync(out, { recursive: true })

const root = mkdtempSync(join(tmpdir(), `runner-export-`))
cpSync(join(import.meta.dirname, `..`, `dist`), root, { recursive: true })
let file = JSON.parse(readFileSync(pkgPath, `utf8`))
// `--core`: the Exponential-app samples use only core components; run them unsigned on the core catalog.
if (values.core) file = { ...(file.package ?? file), catalogId: `https://ui.exponential.at/catalogs/core/v1` }
const pkg = file.package ?? file
writeFileSync(join(root, `app/package.app.json`), JSON.stringify(file))
writeFileSync(join(root, `app/runner.json`), JSON.stringify({ id: `at.exponential.sample.${String(pkg.id).replace(/[^a-zA-Z0-9_.]/g, `_`)}`, name: values.title ?? pkg.name, version: `1.0.0` }))

// `vite preview` over the swapped export (a child process: Playwright and an
// in-process server share Bun's loop badly).
const port = 4400 + Math.floor(Math.random() * 500)
const preview = Bun.spawn([`bunx`, `vite`, `preview`, `--outDir`, root, `--port`, String(port), `--strictPort`], { cwd: join(import.meta.dirname, `..`), stdout: `ignore`, stderr: `inherit` })
for (let i = 0; i < 100; i++) {
  if (await fetch(`http://localhost:${port}/`).then((r) => r.ok, () => false)) break
  await Bun.sleep(100)
}
const server = { port, stop: () => preview.kill() }

const browser = await chromium.launch()
for (const [factor, viewport] of [[`web`, { width: 1280, height: 800 }], [`web-mobile`, { width: 390, height: 844 }]] as const) {
  const context = await browser.newContext({ viewport, deviceScaleFactor: 2, isMobile: factor === `web-mobile`, hasTouch: factor === `web-mobile` })
  const page = await context.newPage()
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => m.type() === `error` && errors.push(m.text()))
  await page.goto(`http://localhost:${server.port}/${values.screen ? `?screen=${values.screen}` : ``}`)
  await page.waitForSelector(`#root *`, { timeout: 20_000 })
  await page.waitForTimeout(1500)
  await page.screenshot({ path: join(out, `${factor}-${name}.png`) })
  if (values.permissions) {
    if (factor === `web`) await page.getByText(`Permissions`, { exact: true }).first().click({ timeout: 5000 })
    else await page.locator(`[data-xui-id="runner-permissions"], #runner-permissions, button[aria-label]`).last().click({ timeout: 5000 })
    await page.waitForTimeout(800)
    await page.screenshot({ path: join(out, `${factor}-${name}-permissions.png`) })
  }
  const backend = await page.evaluate(() => (globalThis as { __runnerBackend?: string }).__runnerBackend ?? null)
  console.log(factor, name, `backend:`, backend, errors.length ? `errors: ${errors.join(` | `)}` : `no errors`)
  await context.close()
}
await browser.close()
server.stop()
