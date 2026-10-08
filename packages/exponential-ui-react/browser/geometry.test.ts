// VAPP-87 acceptance: the layout geometry fixture in headless Chromium —
// the browser's frames (fixed fake measure) equal taffy's, 900 and 390 px,
// LTR and RTL, within the fixture's tolerance (1 px; Blink's 1/64 px
// LayoutUnit is the only delta the spike ever saw).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import geometry from "@exponential-at/ui/fixtures/layout-geometry.json"
import { openPage, startHarness, type Harness } from "./support"

type Frame = { id: string; x: number; y: number; w: number; h: number }
const cases = geometry.cases as Record<string, { width: number; direction: string; frames: Frame[] }>
const tolerance = geometry.measure.tolerancePx

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

describe(`layout-geometry.json in Chromium`, () => {
  for (const [name, c] of Object.entries(cases)) {
    it(`${name}: every node within ±${tolerance}px of taffy`, async () => {
      const page = await openPage(h, { view: `geometry`, case: name })
      const frames = (await page.evaluate(() => window.__xuiFrames!())) as Frame[]
      const byId = new Map(frames.map((f) => [f.id, f]))
      const bad: string[] = []
      let max = 0
      for (const t of c.frames) {
        const b = byId.get(t.id)
        if (!b) {
          bad.push(`${t.id}: missing`)
          continue
        }
        for (const axis of [`x`, `y`, `w`, `h`] as const) {
          const d = Math.abs(b[axis] - t[axis])
          max = Math.max(max, d)
          if (d > tolerance) bad.push(`${t.id}.${axis} taffy=${t[axis]} browser=${b[axis].toFixed(3)}`)
        }
      }
      await page.context().close()
      expect(bad, `${name}: max |delta| ${max.toExponential(2)}`).toEqual([])
      expect(frames.length).toBe(c.frames.length)
    })
  }
})
