// #EXP-2047: a HOSTED popup follows its anchor (interactions-round1.json
// `overlayAnchor`, the hosted cases) in a real browser: the harness's
// bottom panel (`?view=hosted&tick=1`) hosts its calendar against the clock,
// and the clock going 9:59 → 10:00 widens the button (its left edge moves).
// The calendar stays open, its box placed again by the host at the clock's
// new edge. EXP2047_SHOT=<path> saves the panel after the tick.

import { afterAll, beforeAll, expect, it } from "vitest"
import { openPage, startHarness, type Harness } from "./support"

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

it(`a hosted calendar stays open when its clock widens`, async () => {
  const page = await openPage(h, { view: `hosted`, tick: `1`, theme: `exponential`, mode: `dark` }, { width: 900, height: 400 })
  await page.waitForSelector(`[data-xui-hosted="calendar"] [data-xui-overlay="Popover"]`)
  const clock = () => page.evaluate(() => document.querySelector<HTMLElement>(`[data-xui-id="clock"]`)!.getBoundingClientRect().x)
  const before = await clock()
  await page.waitForFunction(() => document.querySelector(`[data-xui-id="clock"]`)?.textContent?.includes(`10:00`))
  await page.waitForTimeout(300)
  if (process.env.EXP2047_SHOT) await page.screenshot({ path: process.env.EXP2047_SHOT })
  expect(await clock(), `the clock's left edge moved`).not.toBe(before)
  expect(await page.$(`[data-xui-hosted="calendar"] [data-xui-overlay="Popover"]`), `the calendar stays open`).not.toBeNull()
  expect(await page.$(`[data-xui-hosted="startMenu"] [data-xui-overlay="Popover"]`), `the start menu (nothing moved) too`).not.toBeNull()
  await page.context().close()
})
