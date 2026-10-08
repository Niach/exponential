// VAPP-87 acceptance: the overlay fixture — a Popover on a Radix portal lands
// where the core's placement rule (`placeOverlay`) says, same side and flip
// result, position within tolerance.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { placeOverlay } from "@exponential-at/ui"
import overlays from "@exponential-at/ui/fixtures/overlay-geometry.json"
import { openPage, startHarness, type Harness } from "./support"

type Case = { name: string; anchor: { x: number; y: number; width: number; height: number }; size: { width: number; height: number }; viewport: { width: number; height: number }; side: `top` | `right` | `bottom` | `left`; expected: { x: number; y: number; side: string; flipped: boolean } }
const cases = overlays.cases as Case[]
const tolerance = overlays.tolerancePx as number

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

describe(`overlay-geometry.json in Chromium`, () => {
  cases.forEach((c, i) => {
    it(`${c.name}: side ${c.expected.side}${c.expected.flipped ? ` (flipped)` : ``}`, async () => {
      const page = await openPage(h, { view: `overlay`, case: i }, { width: c.viewport.width, height: c.viewport.height })
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await page.waitForTimeout(150)
      const got = (await page.evaluate(() => window.__xuiOverlay!())) as { x: number; y: number; w: number; h: number; side: string }
      await page.context().close()
      expect(got).not.toBeNull()
      // The content's own box (the child + the recipe's padding) is what
      // Radix placed; the rule is replayed with that measured size.
      const want = placeOverlay(c.anchor, { width: got.w, height: got.h }, c.viewport, { side: c.side })
      expect(got.side).toBe(c.expected.side)
      expect(want.side).toBe(c.expected.side)
      expect(Math.abs(got.x - want.x), `x: radix ${got.x} rule ${want.x}`).toBeLessThanOrEqual(tolerance)
      expect(Math.abs(got.y - want.y), `y: radix ${got.y} rule ${want.y}`).toBeLessThanOrEqual(tolerance)
    })
  })
})
