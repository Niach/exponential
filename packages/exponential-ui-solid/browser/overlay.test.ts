// FEED-128 (React's browser/overlay.test.ts): the overlay fixture — a
// Popover in the overlay layer lands where the core's placement rule
// (`placeOverlay`) says, same side and flip result, position within
// tolerance, and the content wears the placed `data-side` the recipe
// sheet reads. Also the shared conformance runner's `overlay` block
// (conformance/browser-suites.ts, through `window.__xuiOverlay`).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser } from "playwright"
import { placeInsetOverlay, placeOverlay } from "@exponential_app/eui"
import overlays from "@exponential_app/eui/fixtures/overlay-geometry.json"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

type Case = { name: string; anchor: { x: number; y: number; width: number; height: number }; size: { width: number; height: number }; viewport: { width: number; height: number }; side: `top` | `right` | `bottom` | `left`; align?: `start` | `center` | `end`; matchAnchorWidth?: boolean; expected: { x: number; y: number; side: string; flipped: boolean } }
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
      const want = placeOverlay(c.anchor, { width: got.w, height: got.h }, c.viewport, { side: c.side, align: c.align })
      expect(got.side).toBe(c.expected.side)
      expect(want.side).toBe(c.expected.side)
      expect(Math.abs(got.x - want.x), `x: solid ${got.x} rule ${want.x}`).toBeLessThanOrEqual(tolerance)
      expect(Math.abs(got.y - want.y), `y: solid ${got.y} rule ${want.y}`).toBeLessThanOrEqual(tolerance)
      // `matchAnchorWidth`: the panel is exactly the anchor's width.
      if (c.matchAnchorWidth) expect(Math.abs(got.w - c.anchor.width), `width: ${got.w} anchor ${c.anchor.width}`).toBeLessThanOrEqual(tolerance)
      expect(placed).toEqual({ wrapper: true, position: `fixed`, inLayer: true, align: c.align ?? `center` })
    })
  })
})

// #EXP-1629: the `inset` cases, a pill Select (the plain and the searchable
// popup) EXACTLY where `placeInsetOverlay` puts it, with the measured pill
// and popup: the inset, the clamp, then whole px from the pill (no
// tolerance; half a px is a device px at 2x).
type InsetCase = { name: string; anchor: { x: number; y: number }; viewport: { width: number; height: number }; side: `bottom`; align: `start` | `end`; offset: number; padding: number; inset: number; expected: { side: string } }
describe(`overlay-geometry.json inset (a pill Select) in Chromium`, () => {
  ;(overlays.inset as InsetCase[]).forEach((c, i) => {
    for (const searchable of [false, true]) {
      it(`${c.name}${searchable ? ` (searchable)` : ``}`, async () => {
        const page = await openPage(browser, served.baseUrl, { view: `inset`, case: i, searchable: searchable ? 1 : 0 }, c.viewport)
        await page.click(`[role="combobox"]`)
        await page.waitForSelector(`[data-xui-overlay="Select"]`)
        // The popup's open animation (a scale) settles before it is measured.
        await page.evaluate(() => Promise.all(document.querySelector(`[data-xui-overlay="Select"]`)!.getAnimations({ subtree: true }).map((a) => a.finished)))
        await page.waitForTimeout(150)
        const got = (await page.evaluate(() => (window as unknown as { __xuiOverlay: () => unknown }).__xuiOverlay())) as { x: number; y: number; w: number; h: number; side: string; anchor: { x: number; y: number; width: number; height: number } }
        await page.context().close()
        expect(got).not.toBeNull()
        expect(got.anchor.x).toBe(c.anchor.x)
        expect(got.anchor.y).toBe(c.anchor.y)
        const want = placeInsetOverlay(got.anchor, { width: got.w, height: got.h }, c.viewport, { side: c.side, align: c.align, offset: c.offset, padding: c.padding }, c.inset)
        expect(want.side).toBe(c.expected.side)
        expect(got.side).toBe(c.expected.side)
        expect(got.x, `x`).toBeCloseTo(want.x, 3)
        expect(got.y, `y`).toBeCloseTo(want.y, 3)
      })
    }
  })
})
