// EXP-1604: the shared web Motion painter (React, Solid and this element all
// paint through it) against the raster's references, case by case
// (exponential-ui/conformance/motion-parity.ts; `bun run --filter
// @exponential_app/eui motion:parity` prints the per-case numbers).

import { afterAll, beforeAll, expect, it } from "vitest"
import type { Browser } from "playwright"
import { launchChromium } from "../../exponential-ui/conformance/browser-page"
import { motionParityFloor, runMotionFilterParity, runMotionParity, type MotionFilterParityResult, type MotionParityResult } from "../../exponential-ui/conformance/motion-parity"

const MOTION_FILTER_MIN_SHARE = 0.99

let browser: Browser
let results: MotionParityResult[] = []
let filters: MotionFilterParityResult[] = []
beforeAll(async () => {
  browser = await launchChromium()
  const page = await browser.newPage()
  await page.setContent(`<!doctype html><html><body></body></html>`)
  results = await runMotionParity(page)
  filters = await runMotionFilterParity(page)
})
afterAll(async () => {
  await browser?.close()
})

it(`paints every motion-frames case within 2/255 on ≥ 99 % of pixels (or its known-gap floor)`, () => {
  expect(results.length).toBeGreaterThanOrEqual(36)
  const below = results.filter((r) => r.share < motionParityFloor(r.name)).map((r) => `${r.name}: ${(r.share * 100).toFixed(2)} %`)
  expect(below).toEqual([])
})

it(`blur and drop shadow on ctx.filter paint within 2/255 of the pixel ports on ≥ 99 % of pixels`, () => {
  expect(filters.length).toBeGreaterThan(0)
  expect(filters.every((r) => r.native)).toBe(true)
  const below = filters.filter((r) => r.share < MOTION_FILTER_MIN_SHARE).map((r) => `${r.name} @${r.size}: ${(r.share * 100).toFixed(2)} %`)
  expect(below).toEqual([])
})
