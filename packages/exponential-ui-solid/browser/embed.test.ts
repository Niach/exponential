// #EXP-1769: embed.json's `layout` cases in headless Chromium, React (the
// reference) and Solid: an Embed inside a column takes the column's width
// and its nested surface FILLS it (the card's root spans the Embed within
// 1 px), so its text paints on one line instead of one letter per line
// (the row-flex Embed box around an inline-size container collapsed it to
// 0 px).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser } from "playwright"
import fixture from "@exponential_app/eui/fixtures/embed.json"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

type Case = { name: string; width: number; root: unknown; fills: { embed: string; inner: string }[]; oneLine: string[] }
type Box = { x: number; w: number; h: number; line: number }

const packages = join(import.meta.dirname, `..`, `..`)
let browser: Browser
let react: RunningHarness
let solid: RunningHarness

beforeAll(async () => {
  ;[react, solid] = await Promise.all([bunHarness(join(packages, `exponential-ui-react`, `harness`, `serve.ts`)).start(), bunHarness(join(packages, `exponential-ui-solid`, `harness`, `serve.ts`)).start()])
  browser = await launchChromium()
})

afterAll(async () => {
  await browser?.close()
  await Promise.all([react?.stop(), solid?.stop()])
})

async function measure(h: RunningHarness, c: Case): Promise<Record<string, Box | null>> {
  const page = await openPage(browser, h.baseUrl, { view: `tree`, tree: JSON.stringify(c.root), theme: `neutral`, mode: `light`, width: String(c.width) }, { width: c.width, height: 900 })
  try {
    await page.waitForSelector(`[data-xui-c="Embed"] .xui-surface [data-xui-id]`)
    const ids = [...c.fills.flatMap((f) => [f.embed, `${f.embed}>${f.inner}`]), ...c.oneLine]
    return await page.evaluate((ids) => {
      const out: Record<string, { x: number; w: number; h: number; line: number } | null> = {}
      for (const id of ids) {
        const [outer, inner] = id.split(`>`)
        const sel = inner ? `[data-xui-id="${outer}"] .xui-surface [data-xui-id="${inner}"]` : `[data-xui-c="Embed"] .xui-surface [data-xui-id="${outer}"], [data-xui-id="${outer}"]`
        const el = document.querySelector(sel) as HTMLElement | null
        if (!el) {
          out[id] = null
          continue
        }
        const r = el.getBoundingClientRect()
        out[id] = { x: r.x, w: r.width, h: r.height, line: parseFloat(getComputedStyle(el).lineHeight) }
      }
      return out
    }, ids)
  } finally {
    await page.context().close()
  }
}

describe(`embed.json layout (#EXP-1769)`, () => {
  for (const c of fixture.layout as Case[]) {
    for (const [renderer, h] of [[`react`, () => react], [`solid`, () => solid]] as const) {
      it(`${renderer}: ${c.name}`, async () => {
        const boxes = await measure(h(), c)
        const bad: string[] = []
        for (const f of c.fills) {
          const outer = boxes[f.embed]
          const inner = boxes[`${f.embed}>${f.inner}`]
          if (!outer || !inner) {
            bad.push(`${f.embed}/${f.inner}: missing`)
            continue
          }
          if (outer.w < 100) bad.push(`${f.embed}: Embed ${outer.w.toFixed(1)} px wide`)
          if (Math.abs(inner.x - outer.x) > 1 || Math.abs(inner.w - outer.w) > 1) bad.push(`${f.inner}: x ${inner.x.toFixed(1)} w ${inner.w.toFixed(1)}, Embed x ${outer.x.toFixed(1)} w ${outer.w.toFixed(1)}`)
        }
        for (const id of c.oneLine) {
          const t = boxes[id]
          if (!t) bad.push(`${id}: missing`)
          else if (t.h >= t.line * 1.5) bad.push(`${id}: ${t.h.toFixed(1)} px tall (line ${t.line} px)`)
        }
        expect(bad).toEqual([])
      })
    }
  }
})
