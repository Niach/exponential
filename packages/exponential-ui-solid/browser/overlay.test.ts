// FEED-128 (React's browser/overlay.test.ts): the overlay fixture — a
// Popover in the overlay layer lands where the core's placement rule
// (`placeOverlay`) says, same side and flip result, position within
// tolerance, and the content wears the placed `data-side` the recipe
// sheet reads. Also the shared conformance runner's `overlay` block
// (conformance/browser-suites.ts, through `window.__xuiOverlay`).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser } from "playwright"
import { placeOverlay } from "@exponential_app/eui"
import overlays from "@exponential_app/eui/fixtures/overlay-geometry.json"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

type Case = { name: string; anchor: { x: number; y: number; width: number; height: number }; size: { width: number; height: number }; viewport: { width: number; height: number }; side: `top` | `right` | `bottom` | `left`; expected: { x: number; y: number; side: string; flipped: boolean } }
const cases = overlays.cases as Case[]
const tolerance = overlays.tolerancePx as number

let browser: Browser
let served: RunningHarness
beforeAll(async () => {
  served = await bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start()
  browser = await launchChromium()
})
afterAll(async () => {
  await browser?.close()
  await served?.stop()
})

describe(`overlay-geometry.json in Chromium (Solid)`, () => {
  cases.forEach((c, i) => {
    it(`${c.name}: side ${c.expected.side}${c.expected.flipped ? ` (flipped)` : ``}`, async () => {
      const page = await openPage(browser, served.baseUrl, { view: `overlay`, case: i }, { width: c.viewport.width, height: c.viewport.height })
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await page.waitForTimeout(150)
      const got = (await page.evaluate(() => (window as unknown as { __xuiOverlay: () => unknown }).__xuiOverlay())) as { x: number; y: number; w: number; h: number; side: string }
      const placed = await page.evaluate(() => {
        const el = document.querySelector<HTMLElement>(`[data-xui-overlay="Popover"]`)!
        const wrapper = el.parentElement!
        return { wrapper: wrapper.hasAttribute(`data-radix-popper-content-wrapper`), position: getComputedStyle(wrapper).position, inLayer: Boolean(el.closest(`.xui-container [data-xui-layer="overlay"]`)), align: el.dataset.align }
      })
      await page.context().close()
      expect(got).not.toBeNull()
      // The content's own box (the child + the recipe's padding) is what
      // the popper placed; the rule is replayed with that measured size.
      const want = placeOverlay(c.anchor, { width: got.w, height: got.h }, c.viewport, { side: c.side })
      expect(got.side).toBe(c.expected.side)
      expect(want.side).toBe(c.expected.side)
      expect(Math.abs(got.x - want.x), `x: solid ${got.x} rule ${want.x}`).toBeLessThanOrEqual(tolerance)
      expect(Math.abs(got.y - want.y), `y: solid ${got.y} rule ${want.y}`).toBeLessThanOrEqual(tolerance)
      expect(placed).toEqual({ wrapper: true, position: `fixed`, inLayer: true, align: `center` })
    })
  })
})
