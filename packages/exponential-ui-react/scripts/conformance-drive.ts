// Drives the conformance page (scripts/conformance-page.ts) with Playwright:
// one browser context per case at the case's width, deviceScaleFactor 1,
// the page's fonts loaded, then the frame dump (and optionally a full-page
// PNG for the perceptual step). Node-safe (no Bun APIs): the vitest gate
// (browser/conformance.test.ts) and the CLI (scripts/conformance-web.ts)
// share it.

import type { Browser } from "playwright"
import { DUMP_FORMAT, type CaseDump, type ConformanceCase, type Dump } from "../../exponential-ui/conformance/dump"

export const WEB_RENDERER = `react-chromium`

/** Chromium's launch flags: `--font-render-hinting=none` = unhinted,
 *  FRACTIONAL glyph advances (as macOS and hi-DPI browsers lay text out and
 *  as gpui does). Without it Linux headless Chromium rounds every advance to
 *  a whole pixel and text widths drift by up to ~0.5 px per glyph. */
export const CHROMIUM_ARGS = [`--font-render-hinting=none`]

export async function dumpCase(browser: Browser, baseUrl: string, c: ConformanceCase, opts: { screenshot?: string } = {}): Promise<CaseDump> {
  const context = await browser.newContext({ viewport: { width: c.width, height: 900 }, deviceScaleFactor: 1, locale: `en-US`, reducedMotion: `reduce` })
  try {
    // Offline: only the conformance server answers (the fixtures' example.com
    // media must fail the same way on every machine, never load).
    await context.route(`**/*`, (route) => (route.request().url().startsWith(baseUrl) ? route.continue() : route.abort()))
    const page = await context.newPage()
    const errors: string[] = []
    page.on(`pageerror`, (e) => errors.push(String(e)))
    page.on(`console`, (m) => {
      // The aborted example.com media log as resource errors: expected.
      if (m.type() === `error` && !m.text().startsWith(`Failed to load resource`)) errors.push(m.text())
    })
    await page.goto(`${baseUrl}/?case=${encodeURIComponent(c.key)}`)
    await page.waitForFunction(() => Boolean(window.__xuiConformance), undefined, { timeout: 30_000 })
    const failure = await page.evaluate(async () => {
      try {
        await window.__xuiConformance!.ready
        return null
      } catch (e) {
        return String(e)
      }
    })
    if (failure) throw new Error(`${c.key}: ${failure}`)
    if (errors.length) throw new Error(`${c.key}: page errors:\n${errors.join(`\n`)}`)
    const dump = (await page.evaluate(() => window.__xuiConformance!.dump())) as CaseDump
    if (opts.screenshot) await page.locator(`.xui-surface`).screenshot({ path: opts.screenshot, animations: `disabled` })
    return dump
  } finally {
    await context.close()
  }
}

export async function dumpWeb(browser: Browser, baseUrl: string, cases: ConformanceCase[], onCase?: (c: ConformanceCase, d: CaseDump) => void): Promise<Dump> {
  const out: Dump = { format: DUMP_FORMAT, renderer: WEB_RENDERER, fonts: ``, cases: {} }
  for (const c of cases) {
    const d = await dumpCase(browser, baseUrl, c)
    out.cases[c.key] = d
    onCase?.(c, d)
  }
  return out
}
