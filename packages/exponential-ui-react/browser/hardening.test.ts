// Round 1 renderer-hardening FIX lane, the Chromium half (the jsdom half is
// src/hardening.test.tsx): windowing an UNBOUNDED List (the page scrolls,
// not the List), a bounded one still windowing itself, the keyboard through
// a WINDOWED Table (a roving tab stop; arrows/End by index past the mounted
// rows), height/orientation conditions read the viewport (no feedback
// loop), and two hover Popovers (focus into B's content keeps B open while A is
// open too).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import type { Page } from "playwright"
import { openPage, startHarness, type Harness } from "./support"

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

const treeView = (node: unknown, data: unknown = {}, extra: Record<string, string | number> = {}) => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700, ...extra })
const settle = (page: Page, ms = 150) => page.waitForTimeout(ms)

describe(`List windowing in Chromium`, () => {
  const list = (style?: Record<string, unknown>) => ({
    id: `root`,
    component: `Box`,
    children: [
      { id: `list`, component: `List`, template: { component: `row`, path: `/posts`, key: `id` }, children: [], style: { width: 600, ...(style ?? {}) } },
      { id: `defs`, component: `Box`, visible: false, children: [{ id: `row`, component: `Text`, props: { text: { path: `title` } } }] },
    ],
  })
  it(`an UNBOUNDED List (the page scrolls) renders a window, and follows the page scroll`, async () => {
    const page = await openPage(h, treeView(list(), {}, { gen: `posts:300` }), { width: 800, height: 600 })
    await page.waitForSelector(`.xui-list-window`, { state: `attached` })
    await settle(page)
    const first = await page.evaluate(() => {
      const w = document.querySelector<HTMLElement>(`.xui-list-window`)!
      return { items: document.querySelectorAll(`.xui-list-item`).length, end: Number(w.dataset.windowEnd), height: w.getBoundingClientRect().height }
    })
    expect(first.end).toBeLessThan(300)
    expect(first.items).toBeLessThan(300)
    expect(first.height).toBeGreaterThan(3000)
    // Measured rows correct the estimate (the spacer changes height), so
    // scroll to the bottom until it holds.
    for (let i = 0; i < 10; i++) {
      await page.evaluate(() => window.scrollTo({ top: document.documentElement.scrollHeight }))
      await settle(page, 150)
      if ((await page.evaluate(() => document.querySelector<HTMLElement>(`.xui-list-window`)!.dataset.windowEnd)) === `300`) break
    }
    const last = await page.evaluate(() => {
      const w = document.querySelector<HTMLElement>(`.xui-list-window`)!
      return { start: Number(w.dataset.windowStart), end: Number(w.dataset.windowEnd), items: document.querySelectorAll(`.xui-list-item`).length }
    })
    await page.context().close()
    expect(last.end).toBe(300)
    expect(last.start).toBeGreaterThan(0)
    expect(last.items).toBeLessThan(300)
  })
  it(`a BOUNDED List windows against its own scroll`, async () => {
    const page = await openPage(h, treeView(list({ height: 300 }), {}, { gen: `posts:300` }), { width: 800, height: 600 })
    await page.waitForSelector(`.xui-list-window`, { state: `attached` })
    await settle(page)
    const end0 = await page.evaluate(() => Number(document.querySelector<HTMLElement>(`.xui-list-window`)!.dataset.windowEnd))
    for (let i = 0; i < 10; i++) {
      await page.evaluate(() => {
        const l = document.querySelector<HTMLElement>(`[data-xui-id="list"]`)!
        l.scrollTop = l.scrollHeight
      })
      await settle(page, 150)
      if ((await page.evaluate(() => document.querySelector<HTMLElement>(`.xui-list-window`)!.dataset.windowEnd)) === `300`) break
    }
    const end1 = await page.evaluate(() => Number(document.querySelector<HTMLElement>(`.xui-list-window`)!.dataset.windowEnd))
    await page.context().close()
    expect(end0).toBeLessThan(40)
    expect(end1).toBe(300)
  })
})

describe(`Table keyboard (a11y.json) through a WINDOWED body`, () => {
  it(`Tab reaches the one row stop; ArrowDown walks past the mounted rows; End/Home jump by index`, async () => {
    const rows = Array.from({ length: 120 }, (_, i) => ({ id: `r${i}`, name: `Row ${i}` }))
    const node = { id: `t`, component: `Table`, props: { columns: [{ key: `name`, label: `Name` }], rows, selectable: `single` }, on: { rowPress: { event: { name: `open` } } } }
    const page = await openPage(h, treeView(node), { width: 800, height: 600 })
    await page.waitForSelector(`[data-windowed="true"]`)
    const focusedKey = () => page.evaluate(() => (document.activeElement as HTMLElement | null)?.getAttribute(`data-key`) ?? null)
    expect(await page.locator(`[role="row"][tabindex="0"]`).count()).toBe(1)
    await page.keyboard.press(`Tab`)
    expect(await focusedKey()).toBe(`r0`)
    for (let i = 0; i < 70; i++) await page.keyboard.press(`ArrowDown`)
    await settle(page)
    expect(await focusedKey()).toBe(`r70`)
    await page.keyboard.press(`End`)
    await settle(page, 300)
    expect(await focusedKey()).toBe(`r119`)
    await page.keyboard.press(`ArrowUp`)
    await settle(page)
    expect(await focusedKey()).toBe(`r118`)
    await page.keyboard.press(`Home`)
    await settle(page, 300)
    expect(await focusedKey()).toBe(`r0`)
    await page.keyboard.press(`Enter`)
    await settle(page)
    const log = (await page.evaluate(() => window.__xuiLog)) as { action: string; context: { key: string } }[]
    await page.context().close()
    expect(log.find((l) => l.action === `open`)?.context.key).toBe(`r0`)
  })
})

describe(`height/orientation conditions read the viewport, not the content`, () => {
  it(`tall content: landscape without a host height (unknown, like gpui); with viewportHeight a min-height rule holds and does not oscillate`, async () => {
    const filler = Array.from({ length: 60 }, (_, i) => ({ id: `f${i}`, component: `Text`, props: { text: `Line ${i}` }, style: { height: 50 } }))
    const node = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `wide`, component: `Text`, props: { text: `wide` }, style: { "@media (orientation: landscape)": { fontWeight: 700 } } },
        { id: `tall`, component: `Text`, props: { text: `tall` }, style: { "@media (min-height: 600px)": { display: `none` } } },
        ...filler,
      ],
    }
    const sample = (page: Page) => page.evaluate(() => ({ xq: document.querySelector<HTMLElement>(`.xui-surface`)!.dataset.xq ?? ``, weight: getComputedStyle(document.querySelector(`[data-xui-id="wide"]`)!).fontWeight, tall: getComputedStyle(document.querySelector(`[data-xui-id="tall"]`)!).display }))
    const unknown = await openPage(h, { ...treeView(node), width: 1280 }, { width: 1300, height: 650 })
    await settle(unknown, 300)
    const u = await sample(unknown)
    await unknown.context().close()
    expect(u.xq).toBe(`orientation-landscape`)
    expect(u.weight).toBe(`700`)
    expect(u.tall).not.toBe(`none`)
    const page = await openPage(h, { ...treeView(node), width: 1280, viewportHeight: 650 }, { width: 1300, height: 650 })
    await settle(page, 300)
    const a = await sample(page)
    await settle(page, 400)
    const b = await sample(page)
    await page.context().close()
    expect(a.xq).toBe(`orientation-landscape min-height-600px`)
    expect(a.weight).toBe(`700`)
    expect(a.tall).toBe(`none`)
    expect(b).toEqual(a)
  })
  it(`viewportHeight from the host decides (a 400-tall panel: max-height holds, portrait at 300 wide)`, async () => {
    const node = { id: `n`, component: `Text`, props: { text: `p` }, style: { "@media (orientation: portrait)": { fontWeight: 700 } } }
    const page = await openPage(h, { ...treeView(node), width: 300, viewportHeight: 400 }, { width: 1200, height: 700 })
    await settle(page)
    const weight = await page.evaluate(() => getComputedStyle(document.querySelector(`[data-xui-id="n"]`)!).fontWeight)
    await page.context().close()
    expect(weight).toBe(`700`)
  })
})

describe(`a hover Popover's focus is scoped to its OWN content`, () => {
  it(`with A open, focus moving from B's trigger into B's content keeps B open`, async () => {
    const card = (k: string) => ({ id: `hc${k}`, component: `Popover`, props: { openOn: `hover` }, slots: { trigger: { id: `t${k}`, component: `Button`, props: { label: `Card ${k}` } } }, children: [{ id: `b${k}`, component: `Button`, props: { label: `Inside ${k}` } }] })
    const node = { id: `root`, component: `Box`, style: { display: `flex`, gap: 200, padding: 80 }, children: [card(`A`), card(`B`)] }
    const page = await openPage(h, treeView(node, {}, { width: 900 }), { width: 1000, height: 600 })
    await page.hover(`[data-xui-id="tA"]`)
    await page.waitForSelector(`[data-xui-overlay="Popover"]:has([data-xui-id="bA"])`, { timeout: 3000 })
    await page.hover(`[data-xui-id="bA"]`) // the pointer rests on A's content: A stays
    await page.focus(`[data-xui-id="tB"]`)
    await page.waitForSelector(`[data-xui-overlay="Popover"]:has([data-xui-id="bB"])`, { timeout: 3000 })
    expect(await page.locator(`[data-xui-overlay="Popover"]`).count()).toBe(2)
    await page.focus(`[data-xui-id="bB"]`)
    await settle(page, 500)
    const open = await page.locator(`[data-xui-overlay="Popover"]:has([data-xui-id="bB"])`).count()
    await page.context().close()
    expect(open).toBe(1)
  })
})
