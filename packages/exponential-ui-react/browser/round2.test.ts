// Round 2 (packages/exponential-ui/docs/round-2-contract.md), the Chromium
// half (src/round2.test.tsx is the jsdom half): Resizable drag + keys on
// real boxes (panel extents = panelExtents), sticky section headers pinned
// and pushed back, a horizontal List windowing on its own axis,
// scrollToIndex, the animation / backdrop-blur CSS and reduced motion, the
// §7 boxes (overlay = its trigger, Video 16:9, Chart = height).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { panelExtents } from "@exponential-at/ui"
import type { Page } from "playwright"
import { openPage, startHarness, type Harness } from "./support"

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

const box = (page: Page, id: string) =>
  page.evaluate((id) => {
    const r = document.querySelector(`[data-xui-id="${id}"]`)!.getBoundingClientRect()
    return { x: r.x, y: r.y, w: r.width, h: r.height }
  }, id)
const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))

describe(`Resizable`, () => {
  it(`panels take panelExtents of the group; a drag moves the handle with the pointer; keys step 10`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    const group = await box(page, `rz`)
    const want = panelExtents([30, 70], group.w, 1)
    expect(Math.abs((await box(page, `rz.panel.0`)).w - want[0])).toBeLessThanOrEqual(1)
    expect(Math.abs((await box(page, `rz.panel.1`)).w - want[1])).toBeLessThanOrEqual(1)
    const handle = await box(page, `rz.handle.0`)
    await page.mouse.move(handle.x + 0.5, handle.y + 60)
    await page.mouse.down()
    await page.mouse.move(handle.x + 0.5 + (group.w - 1) * 0.2, handle.y + 60, { steps: 5 })
    await page.mouse.up()
    await frame(page)
    const after = await page.evaluate(() => Number(document.querySelector<HTMLElement>(`[data-xui-id="rz.panel.0"]`)!.dataset.size))
    expect(after).toBeCloseTo(50, 0)
    await page.focus(`[data-xui-id="rz.handle.0"]`)
    await page.keyboard.press(`ArrowLeft`)
    await frame(page)
    expect(await page.evaluate(() => Number(document.querySelector<HTMLElement>(`[data-xui-id="rz.panel.0"]`)!.dataset.size))).toBeCloseTo(after - 10, 3)
    await page.context().close()
  })
})

describe(`List: sticky headers, the horizontal axis, scrollToIndex`, () => {
  it(`the current section header pins at the top of a bounded list, the next one pushes it back`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    const list = await box(page, `sec`)
    // Section 0 has 40 rows of 40 px (+ 1 px dividers); scroll into it.
    await page.evaluate(() => (document.querySelector(`[data-xui-id="sec"]`)!.scrollTop = 600))
    await frame(page)
    const head = await box(page, `sec.section.0`)
    expect(Math.abs(head.y - list.y)).toBeLessThanOrEqual(1)
    // Section 1 starts at 32 + 40 × 41 − 1 + 1; just before it the pinned header is pushed up.
    await page.evaluate(() => (document.querySelector(`[data-xui-id="sec"]`)!.scrollTop = 32 + 40 * 41 - 16))
    await frame(page)
    expect((await box(page, `sec.section.0`)).y).toBeLessThan(list.y)
    await page.context().close()
  })
  it(`a horizontal list renders a window and follows its own horizontal scroll`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    const before = await page.evaluate(() => Number(document.querySelector<HTMLElement>(`[data-xui-id="hl"] .xui-list-window`)!.dataset.windowEnd))
    expect(before).toBeLessThan(40)
    await page.evaluate(() => (document.querySelector(`[data-xui-id="hl"]`)!.scrollLeft = 88 * 200))
    await frame(page)
    await frame(page)
    const got = await page.evaluate(() => {
      const scroller = document.querySelector(`[data-xui-id="hl"]`)!
      const w = scroller.querySelector<HTMLElement>(`.xui-list-window`)!
      const s = scroller.getBoundingClientRect()
      const visible = Array.from(w.querySelectorAll(`.xui-list-item`)).filter((el) => {
        const r = el.getBoundingClientRect()
        return r.right > s.left && r.left < s.right
      }).length
      return { start: Number(w.dataset.windowStart), rendered: w.querySelectorAll(`.xui-list-item`).length, visible }
    })
    // Unmeasured chips are estimated at $control.row: the window lands where
    // the estimates put it and the items there are on screen.
    expect(got.start).toBeGreaterThan(100)
    expect(got.rendered).toBeLessThan(40)
    expect(got.visible).toBeGreaterThan(2)
    await page.context().close()
  })
  it(`scrollToIndex {align: start} puts the item under the pinned header`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    await page.evaluate(() => window.__xuiHandle!()!.run({ scrollToIndex: { id: `sec`, index: 100, align: `start` } }))
    await frame(page)
    await frame(page)
    const list = await box(page, `sec`)
    const item = await box(page, `sec-row.i100`)
    const pinned = await page.evaluate(() => document.querySelector(`[data-pinned]`)?.getBoundingClientRect().height ?? 0)
    expect(Math.abs(item.y - (list.y + pinned))).toBeLessThanOrEqual(1)
    await page.context().close()
  })
})

describe(`style keys and §7 boxes`, () => {
  it(`animation + backdrop blur compute; reduced motion zeroes the duration`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    const css = await page.evaluate(() => {
      const cs = getComputedStyle(document.querySelector(`[data-xui-id="anim"]`)!)
      return { name: cs.animationName, iterations: cs.animationIterationCount, blur: cs.backdropFilter, duration: cs.animationDuration }
    })
    expect(css).toMatchObject({ name: `xui-pulse`, iterations: `infinite`, blur: `blur(12px)` })
    expect(parseFloat(css.duration)).toBeGreaterThan(1)
    await page.emulateMedia({ reducedMotion: `reduce` })
    await frame(page)
    expect(await page.evaluate(() => getComputedStyle(document.querySelector(`[data-xui-id="anim"]`)!).animationDuration)).toBe(`0s`)
    await page.context().close()
  })
  it(`an overlay in a column is its trigger's box; Video is 16:9; Chart's frame is its height`, async () => {
    const page = await openPage(h, { view: `round2` }, { width: 390, height: 900 })
    expect(await box(page, `dr`)).toEqual(await box(page, `dr-t`))
    const vid = await box(page, `vid`)
    expect(Math.abs(vid.h - vid.w / 1.7777778)).toBeLessThanOrEqual(1)
    expect((await box(page, `ch`)).h).toBe(180)
    await page.context().close()
  })
})
