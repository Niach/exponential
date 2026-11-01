// EXP-1604: the shared web Motion painter (React, Solid and this element all
// paint through it) against the raster's references, case by case
// (exponential-ui/conformance/motion-parity.ts; `bun run --filter
// @exponential_app/eui motion:parity` prints the per-case numbers).

import { afterAll, beforeAll, expect, it } from "vitest"
import type { Browser } from "playwright"
import { launchChromium } from "../../exponential-ui/conformance/browser-page"
import { motionParityFloor, runMotionParity, type MotionParityResult } from "../../exponential-ui/conformance/motion-parity"

let browser: Browser
let results: MotionParityResult[] = []
beforeAll(async () => {
  browser = await launchChromium()
  const page = await browser.newPage()
  await page.setContent(`<!doctype html><html><body></body></html>`)
  results = await runMotionParity(page)
})
afterAll(async () => {
  await browser?.close()
})

it(`paints every motion-frames case within 2/255 on ≥ 99 % of pixels (or its known-gap floor)`, () => {
  expect(results.length).toBeGreaterThanOrEqual(36)
  const below = results.filter((r) => r.share < motionParityFloor(r.name)).map((r) => `${r.name}: ${(r.share * 100).toFixed(2)} %`)
  expect(below).toEqual([])
})
