// VAPP-87: shared setup of the headless-Chromium suites — the harness server
// on a free port and one Playwright browser per file.

import { chromium, type Browser, type Page } from "playwright"
import { spawn } from "node:child_process"
import { join } from "node:path"

const pkgRoot = join(import.meta.dirname, `..`)

export interface Harness {
  url: (query: Record<string, string | number | undefined>) => string
  browser: Browser
  close: () => Promise<void>
}

export async function startHarness(): Promise<Harness> {
  const port = 4300 + Math.floor(Math.random() * 500)
  const proc = spawn(`bun`, [join(pkgRoot, `harness/serve.ts`)], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `pipe`, `pipe`] })
  let stderr = ``
  proc.stderr?.on(`data`, (d) => (stderr += String(d)))
  const base = `http://127.0.0.1:${port}`
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${base}/dist/main.js`)
      if (r.ok) break
      if (r.status === 500) throw new Error(await r.text())
    } catch (e) {
      if (String(e).includes(`error`) && !String(e).includes(`ECONNREFUSED`) && !String(e).includes(`fetch`)) throw e
    }
    await new Promise((r) => setTimeout(r, 150))
  }
  const browser = await chromium.launch()
  return {
    browser,
    url: (query) => {
      const q = new URLSearchParams()
      for (const [k, v] of Object.entries(query)) if (v !== undefined) q.set(k, String(v))
      return `${base}/?${q.toString()}`
    },
    close: async () => {
      await browser.close()
      proc.kill()
      if (stderr.trim()) console.error(stderr)
    },
  }
}

export async function openPage(h: Harness, query: Record<string, string | number | undefined>, viewport = { width: 1200, height: 900 }): Promise<Page> {
  const context = await h.browser.newContext({ viewport, deviceScaleFactor: 1 })
  const page = await context.newPage()
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => {
    if (m.type() === `error`) errors.push(m.text())
  })
  await page.goto(h.url(query))
  await page.waitForSelector(`.xui-surface`, { timeout: 30_000, state: `attached` })
  await page.waitForTimeout(250)
  if (errors.length) throw new Error(`page errors: ${errors.join(`\n`)}`)
  return page
}
