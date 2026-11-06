// FEED-128: the Playwright side of the shared WEB conformance tooling — one
// headless Chromium (`EXPONENTIAL_UI_CHROME` = an explicit executable, for a
// machine whose Playwright build is not installed), a renderer HARNESS served
// on a loopback port, and the page opener every painted suite uses. Node-safe
// (vitest runs the runners under Node): a harness is a `bun <serve.ts>`
// child process, never an in-process `Bun.build`.
//
// The harness protocol (packages/exponential-ui-react/harness/main.tsx is
// the reference): `<serve.ts>` honours `PORT`, answers `/` with a page whose
// module script is the bundle, and that page reads `?view=…` (kitchen-sink,
// geometry, overlay, catalog, conditions, tree, controls, specimen, mount,
// bench, round2, primitives = FEED-131's primitives-page.ts; catalog takes
// `shard=k/n`, EXP-1702) plus
// theme/mode/rtl/width/echo and exposes the `window.__xui*` probes.

import { chromium, type Browser, type BrowserContext, type Page } from "playwright"
import { spawn, type ChildProcess } from "node:child_process"
import { createServer } from "node:net"
import { mkdirSync } from "node:fs"
import { dirname, join } from "node:path"

export type Query = Record<string, string | number | undefined>

/** A renderer's harness: `start()` serves its bundle on a loopback port. */
export interface WebHarness {
  start(): Promise<RunningHarness>
}

export interface RunningHarness {
  baseUrl: string
  stop(): void | Promise<void>
}

/** The Chromium executable override (`EXPONENTIAL_UI_CHROME`), else
 *  Playwright's own build. */
export const chromeExecutable = (): string | undefined => process.env.EXPONENTIAL_UI_CHROME || undefined

export async function launchChromium(args: string[] = [], executablePath = chromeExecutable()): Promise<Browser> {
  return chromium.launch({ executablePath, args })
}

/** `${baseUrl}/?<query>` (undefined values dropped). */
export function pageUrl(baseUrl: string, query: Query): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(query)) if (v !== undefined) q.set(k, String(v))
  return `${baseUrl}/?${q.toString()}`
}

/** A free loopback TCP port. */
export function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer()
    srv.unref()
    srv.on(`error`, reject)
    srv.listen(0, `127.0.0.1`, () => {
      const addr = srv.address()
      const port = typeof addr === `object` && addr ? addr.port : 0
      srv.close(() => resolve(port))
    })
  })
}

/** The harness of a `serve.ts` that honours `PORT`: spawned with `bun`,
 *  ready once `/` answers and the module script it names bundles (a 500 =
 *  the bundler's error, thrown). A child that dies before `stop()` (killed
 *  from outside on a busy machine) is respawned on the same port. */
export function bunHarness(serveScript: string, opts: { timeoutMs?: number } = {}): WebHarness {
  return {
    async start() {
      const port = await freePort()
      const baseUrl = `http://127.0.0.1:${port}`
      let stderr = ``
      let stopping = false
      let proc: ChildProcess
      const spawnChild = () => {
        proc = spawn(`bun`, [serveScript], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `pipe`, `pipe`] })
        proc.stderr?.on(`data`, (d) => (stderr += String(d)))
        proc.on(`exit`, (code, signal) => {
          if (stopping) return
          stderr += `${serveScript} exited (${code ?? signal}); respawned\n`
          spawnChild()
        })
      }
      spawnChild()
      const deadline = Date.now() + (opts.timeoutMs ?? 60_000)
      for (;;) {
        let ready = false
        try {
          const page = await fetch(`${baseUrl}/`)
          if (page.ok) {
            const src = /<script[^>]*src="([^"]+)"/.exec(await page.text())?.[1] ?? `/dist/main.js`
            const r = await fetch(new URL(src, baseUrl))
            if (r.status === 500) {
              stopping = true
              proc!.kill()
              throw new Error(`${serveScript}: bundle failed: ${await r.text()}`)
            }
            // Drained: an unread 5 MB body stalls the server's write until
            // Bun.serve's 10 s idle timeout drops it (the EXP-1702 warning).
            await r.arrayBuffer()
            ready = r.ok
          }
        } catch (e) {
          if (String(e).includes(`bundle failed`)) throw e
        }
        if (ready) break
        if (Date.now() > deadline) {
          stopping = true
          proc!.kill()
          throw new Error(`${serveScript} did not start on ${baseUrl}: ${stderr}`)
        }
        await new Promise((r) => setTimeout(r, 150))
      }
      return {
        baseUrl,
        stop: () => {
          stopping = true
          proc.kill()
          if (stderr.trim()) console.error(stderr)
        },
      }
    },
  }
}

export interface OpenPageOptions {
  viewport?: { width: number; height: number }
  /** Wait for a `.xui-surface` (default true). */
  waitForSurface?: boolean
  /** Extra settle time after the surface attached, ms (default 250). */
  settleMs?: number
  context?: Parameters<Browser[`newContext`]>[0]
}

/** A fresh context + page on the harness at `query`; throws on page errors. */
export async function openPage(browser: Browser, baseUrl: string, query: Query, viewport = { width: 1200, height: 900 }, opts: Omit<OpenPageOptions, `viewport`> = {}): Promise<Page> {
  const context: BrowserContext = await browser.newContext({ viewport, deviceScaleFactor: 1, ...opts.context })
  const page = await context.newPage()
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => {
    if (m.type() === `error`) errors.push(m.text())
  })
  // EXP-1702: what the page fetched, so a page that never paints says why.
  const started = Date.now()
  const requests: string[] = []
  page.on(`requestfailed`, (r) => requests.push(`FAILED ${r.url()} ${r.failure()?.errorText ?? ``}`))
  page.on(`requestfinished`, (r) => requests.push(`${Date.now() - started} ms ${r.url()}`))
  await page.goto(pageUrl(baseUrl, query))
  if (opts.waitForSurface !== false) {
    try {
      await page.waitForSelector(`.xui-surface`, { timeout: 30_000, state: `attached` })
    } catch (e) {
      throw new Error(`${e instanceof Error ? e.message : String(e)}\n${await pageDiagnosis(page, query, started, errors, requests)}`)
    }
  }
  await page.waitForTimeout(opts.settleMs ?? 250)
  if (errors.length) {
    await context.close()
    throw new Error(`page errors: ${errors.join(`\n`)}`)
  }
  return page
}

/** EXP-1702: a page that never painted, told: the time, its errors, every
 *  request, the DOM size; plus a screenshot beside the conformance report
 *  (`EXPONENTIAL_UI_CONFORMANCE_REPORT`'s directory, uploaded by CI). */
async function pageDiagnosis(page: Page, query: Query, started: number, errors: string[], requests: string[]): Promise<string> {
  const dom = await page.evaluate(() => ({ nodes: document.querySelectorAll(`*`).length, styles: document.querySelectorAll(`style`).length, surfaces: document.querySelectorAll(`.xui-surface`).length })).catch((e) => String(e))
  let shot = ``
  const report = process.env.EXPONENTIAL_UI_CONFORMANCE_REPORT
  if (report) {
    const path = join(dirname(report), `failures`, `${Object.values(query).filter((v) => v !== undefined).join(`-`).replace(/[^a-z0-9-]/gi, `_`).slice(0, 80)}-${Date.now()}.png`)
    mkdirSync(dirname(path), { recursive: true })
    shot = await page.screenshot({ path, fullPage: false }).then(() => `screenshot: ${path}`, (e) => `screenshot failed: ${e}`)
  }
  return [`page ${pageUrl(``, query)} after ${Date.now() - started} ms; dom ${JSON.stringify(dom)}`, `errors: ${errors.length ? errors.join(` | `) : `none`}`, `requests:`, ...requests.map((r) => `  ${r}`), shot].filter(Boolean).join(`\n`)
}
