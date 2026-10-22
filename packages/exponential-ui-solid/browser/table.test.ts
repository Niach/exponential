// FEED-128 (the Table / ChipInput / FileUpload / SearchList lane): the
// interaction parity in headless Chromium. Every scripted interaction runs on
// the React harness (the reference) AND the Solid one; the `__xuiLog` entries
// (and the DOM state the test reads back) must be IDENTICAL. Covers React's
// browser/hardening.test.ts "Table keyboard through a WINDOWED body", Table
// sort + selection (single row press, multiple checkboxes + select-all),
// ChipInput keys (Enter, comma, Backspace twice, suggestions), FileUpload
// (a drop with dragover, the picker, refusals, remove) and SearchList keys
// (type, ↑/↓, Enter, Tab, hover, the debounced `search`).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { bunHarness, launchChromium, openPage, type Query, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

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
  await react?.stop()
  await solid?.stop()
})

const treeView = (node: unknown, data: unknown = {}): Query => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700 })
const settle = (page: Page, ms = 150) => page.waitForTimeout(ms)
const log = (page: Page) => page.evaluate(() => window.__xuiLog) as Promise<unknown[]>
const focusedKey = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.getAttribute(`data-key`) ?? null)

/** The same script on both harnesses: `{react, solid}` of what it returns. */
async function both<T>(query: Query, script: (page: Page) => Promise<T>, viewport = { width: 800, height: 600 }): Promise<{ react: T; solid: T }> {
  const run = async (h: RunningHarness) => {
    const page = await openPage(browser, h.baseUrl, query, viewport)
    try {
      return await script(page)
    } finally {
      await page.context().close()
    }
  }
  const [r, s] = await Promise.all([run(react), run(solid)])
  return { react: r, solid: s }
}

declare global {
  interface Window {
    __xuiLog: unknown[]
  }
}

const members = { columns: [{ key: `name`, label: `Name`, sortable: true }, { key: `runs`, label: `Runs`, type: `number`, sortable: true, align: `end` }], rows: [{ id: `a`, name: `Alice`, runs: 12 }, { id: `b`, name: `Bob`, runs: 3 }, { id: `c`, name: `Cy`, runs: 7 }] }
const rowOrder = (page: Page) => page.evaluate(() => Array.from(document.querySelectorAll(`[role="rowgroup"] [role="row"][data-key]`)).map((r) => `${r.getAttribute(`data-key`)}:${r.getAttribute(`aria-selected`)}`))

describe(`Table`, () => {
  it(`keyboard through a WINDOWED body: Tab reaches the one row stop; ArrowDown walks past the mounted rows; End/Home jump; Enter presses`, async () => {
    const rows = Array.from({ length: 120 }, (_, i) => ({ id: `r${i}`, name: `Row ${i}` }))
    const node = { id: `t`, component: `Table`, props: { columns: [{ key: `name`, label: `Name` }], rows, selectable: `single` }, on: { rowPress: { event: { name: `open` } } } }
    const out = await both(treeView(node), async (page) => {
      await page.waitForSelector(`[data-windowed="true"]`)
      const seen: unknown[] = [await page.locator(`[role="row"][tabindex="0"]`).count()]
      await page.keyboard.press(`Tab`)
      seen.push(await focusedKey(page))
      for (let i = 0; i < 70; i++) await page.keyboard.press(`ArrowDown`)
      await settle(page)
      seen.push(await focusedKey(page))
      await page.keyboard.press(`End`)
      await settle(page, 300)
      seen.push(await focusedKey(page))
      await page.keyboard.press(`ArrowUp`)
      await settle(page)
      seen.push(await focusedKey(page))
      await page.keyboard.press(`Home`)
      await settle(page, 300)
      seen.push(await focusedKey(page))
      await page.keyboard.press(`Enter`)
      await settle(page)
      return { seen, log: await log(page) }
    })
    expect(out.react.seen).toEqual([1, `r0`, `r70`, `r119`, `r118`, `r0`])
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })

  it(`sort: a sortable header cycles asc → desc (the local order follows), Space selects a single row, a click presses`, async () => {
    const node = { id: `t`, component: `Table`, props: { ...members, selectable: `single` }, on: { sort: { event: { name: `sorted` } }, select: { event: { name: `selected` } }, rowPress: { event: { name: `open` } } } }
    const out = await both(treeView(node), async (page) => {
      const seen: unknown[] = []
      await page.click(`[data-xui-id="t.headerCell.1"] button`)
      await settle(page)
      seen.push(await rowOrder(page), await page.getAttribute(`[data-xui-id="t.headerCell.1"]`, `aria-sort`))
      await page.click(`[data-xui-id="t.headerCell.1"] button`)
      await settle(page)
      seen.push(await rowOrder(page), await page.getAttribute(`[data-xui-id="t.headerCell.1"]`, `aria-sort`))
      await page.click(`[data-xui-id="t.row.c"]`)
      await settle(page)
      seen.push(await rowOrder(page))
      await page.keyboard.press(`ArrowDown`)
      await page.keyboard.press(`Space`)
      await settle(page)
      seen.push(await rowOrder(page), await focusedKey(page))
      await page.keyboard.press(`Space`)
      await settle(page)
      seen.push(await rowOrder(page))
      return { seen, log: await log(page) }
    })
    expect(out.react.seen[0]).toEqual([`b:false`, `c:false`, `a:false`])
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })

  it(`multiple: row checkboxes, the select-all header (indeterminate → all → none), a bound selected`, async () => {
    const node = { id: `t`, component: `Table`, props: { ...members, selectable: `multiple`, selected: { path: `/sel` } }, on: { select: { event: { name: `selected`, context: { bound: { path: `/sel` } } } } } }
    const state = (page: Page) => page.evaluate(() => ({ head: document.querySelector(`[data-xui-id="t.checkbox.header"]`)!.getAttribute(`aria-checked`), rows: Array.from(document.querySelectorAll(`[role="checkbox"]`)).map((c) => c.getAttribute(`data-state`)), live: document.querySelector(`[aria-live="polite"].xui-sr-only`)?.textContent ?? null }))
    const out = await both(treeView(node, { sel: [`b`] }), async (page) => {
      const seen: unknown[] = [await state(page)]
      await page.click(`[data-xui-id="t.checkbox.a"]`)
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="t.checkbox.header"]`)
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="t.checkbox.header"]`)
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="t.row.c"]`)
      await page.focus(`[data-xui-id="t.checkbox.c"]`)
      await page.keyboard.press(`Space`)
      await settle(page)
      seen.push(await state(page))
      return { seen, log: await log(page) }
    })
    expect(out.react.seen[0]).toEqual({ head: `mixed`, rows: [`indeterminate`, `unchecked`, `checked`, `unchecked`], live: `1 selected` })
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })
})

describe(`ChipInput keys`, () => {
  it(`Enter and a comma add, duplicates refuse, Backspace focuses then removes, chip arrows, suggestions ↓ + Enter, Escape`, async () => {
    const node = { id: `c`, component: `ChipInput`, props: { label: `Labels`, values: { path: `/labels` }, suggestions: [{ label: `bug`, value: `bug` }, { label: `bugfix`, value: `bugfix` }, { label: `design`, value: `design` }], max: 5 }, on: { add: { event: { name: `added` } }, remove: { event: { name: `removed` } }, change: { event: { name: `changed`, context: { all: { path: `/labels` } } } } } }
    const chips = (page: Page) => page.evaluate(() => ({ chips: Array.from(document.querySelectorAll(`[data-chip]`)).map((c) => c.textContent), text: document.querySelector<HTMLInputElement>(`[data-xui-id="c"] input`)!.value, focus: document.activeElement?.getAttribute(`data-xui-id`) ?? document.activeElement?.tagName ?? null, expanded: document.querySelector(`[data-xui-id="c"] input`)!.getAttribute(`aria-expanded`), options: Array.from(document.querySelectorAll(`[role="option"]`)).map((o) => `${o.textContent}:${o.getAttribute(`aria-selected`)}`) }))
    const out = await both(treeView(node, { labels: [`ios`] }), async (page) => {
      const input = `[data-xui-id="c"] input`
      const seen: unknown[] = []
      await page.click(input)
      await page.keyboard.type(`alpha`)
      await page.keyboard.press(`Enter`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.type(`beta,gamma,`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.type(`ios`)
      await page.keyboard.press(`Enter`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`Escape`)
      await page.keyboard.type(`bu`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`ArrowDown`)
      await page.keyboard.press(`ArrowDown`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`Enter`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`Backspace`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`ArrowLeft`)
      await settle(page)
      seen.push(await chips(page))
      await page.keyboard.press(`Backspace`)
      await settle(page)
      seen.push(await chips(page))
      await page.click(`[data-xui-id="c.remove.0"]`)
      await settle(page)
      seen.push(await chips(page))
      return { seen, log: await log(page) }
    })
    expect((out.react.seen[1] as { chips: string[] }).chips).toEqual([`ios`, `alpha`, `beta`, `gamma`])
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })
})

describe(`FileUpload`, () => {
  it(`a drop paints dragover then takes accepted files (refusals listed), the picker adds, remove`, async () => {
    const node = { id: `f`, component: `FileUpload`, props: { label: `Files`, name: `files`, accept: `image/*,.pdf`, multiple: true, maxSize: 1000 }, on: { upload: { event: { name: `uploaded` } }, remove: { event: { name: `removedFile` } } } }
    const state = (page: Page) => page.evaluate(() => ({ drag: document.querySelector(`[data-xui-id="f"] [role="button"]`)!.getAttribute(`data-dragover`), xs: document.querySelector(`[data-xui-id="f"] [role="button"]`)!.getAttribute(`data-xs`), files: Array.from(document.querySelectorAll(`.xui-file-list li`)).map((l) => l.textContent), refused: Array.from(document.querySelectorAll(`[role="alert"] .xui-field-error`)).map((e) => e.textContent), described: document.querySelector(`[data-xui-id="f"] [role="button"]`)!.getAttribute(`aria-describedby`) !== null }))
    const out = await both(treeView(node), async (page) => {
      const seen: unknown[] = []
      await page.evaluate(() => {
        const zone = document.querySelector(`[data-xui-id="f"] [role="button"]`)!
        const dt = new DataTransfer()
        dt.items.add(new File([`x`.repeat(10)], `a.png`, { type: `image/png` }))
        dt.items.add(new File([`x`], `notes.txt`, { type: `text/plain` }))
        dt.items.add(new File([`x`.repeat(2000)], `big.pdf`, { type: `application/pdf` }))
        zone.dispatchEvent(new DragEvent(`dragenter`, { bubbles: true, cancelable: true, dataTransfer: dt }))
        zone.dispatchEvent(new DragEvent(`dragover`, { bubbles: true, cancelable: true, dataTransfer: dt }))
        ;(window as unknown as { __dt: DataTransfer }).__dt = dt
      })
      await settle(page)
      seen.push(await state(page))
      await page.evaluate(() => {
        const zone = document.querySelector(`[data-xui-id="f"] [role="button"]`)!
        zone.dispatchEvent(new DragEvent(`drop`, { bubbles: true, cancelable: true, dataTransfer: (window as unknown as { __dt: DataTransfer }).__dt }))
      })
      await settle(page)
      seen.push(await state(page))
      await page.setInputFiles(`[data-xui-id="f"] input[type="file"]`, [{ name: `b.jpg`, mimeType: `image/jpeg`, buffer: Buffer.from(`hello`) }])
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="f.remove.0"]`)
      await settle(page)
      seen.push(await state(page))
      return { seen, log: await log(page) }
    })
    expect((out.react.seen[0] as { drag: string | null }).drag).toBe(``)
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })
})

describe(`SearchList keys`, () => {
  it(`typing filters (top row preselected, the count), ↑/↓ clamp, Enter picks, Tab picks, hover moves, the debounced search`, async () => {
    const items = Array.from({ length: 12 }, (_, i) => ({ label: `Issue ${i}`, value: `v${i}`, identifier: `EXP-${i}`, disabled: i === 3 }))
    const node = { id: `s`, component: `SearchList`, props: { items, value: { path: `/v` }, maxRows: 4 }, on: { select: { event: { name: `picked`, context: { bound: { path: `/v` } } } }, search: { event: { name: `searched` } } } }
    const state = (page: Page) => page.evaluate(() => ({ active: document.querySelector(`[data-active="true"]`)?.getAttribute(`data-xui-id`) ?? null, desc: document.querySelector(`[role="combobox"]`)!.getAttribute(`aria-activedescendant`) === document.querySelector(`[data-active="true"]`)?.id, count: document.querySelector(`[role="status"]`)?.textContent ?? null, rows: document.querySelectorAll(`[role="option"]`).length, checked: document.querySelector(`.xui-SearchList-check`)?.getAttribute(`data-xui-id`) ?? null, fade: Boolean(document.querySelector(`.xui-SearchList-fade`)), scroll: Math.round(document.querySelector<HTMLElement>(`[role="listbox"]`)!.scrollTop) }))
    const out = await both(treeView(node, { v: `` }), async (page) => {
      const seen: unknown[] = [await state(page)]
      await page.click(`[role="combobox"]`)
      for (let i = 0; i < 6; i++) await page.keyboard.press(`ArrowDown`)
      await settle(page)
      seen.push(await state(page))
      await page.keyboard.press(`Enter`)
      await settle(page)
      seen.push(await state(page))
      await page.keyboard.type(`issue 1`)
      await settle(page, 400)
      seen.push(await state(page))
      await page.keyboard.press(`ArrowDown`)
      await page.keyboard.press(`ArrowDown`)
      await page.keyboard.press(`ArrowDown`)
      await page.keyboard.press(`ArrowUp`)
      await settle(page)
      seen.push(await state(page))
      await page.keyboard.press(`Tab`)
      await settle(page)
      seen.push(await state(page))
      await page.fill(`[role="combobox"]`, `zzz`)
      await settle(page, 400)
      seen.push(await state(page))
      await page.keyboard.press(`Enter`)
      await page.fill(`[role="combobox"]`, ``)
      await settle(page, 400)
      await page.hover(`[data-xui-id="s.item.2"]`)
      await page.mouse.move(10, 10)
      await page.hover(`[data-xui-id="s.item.2"]`)
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="s.item.3"]`, { force: true })
      await settle(page)
      seen.push(await state(page))
      await page.click(`[data-xui-id="s.item.5"]`)
      await settle(page)
      seen.push(await state(page))
      return { seen, log: await log(page) }
    })
    expect((out.react.seen[1] as { active: string }).active).toBe(`s.item.6`)
    expect(out.react.log.length).toBeGreaterThan(0)
    expect(out.solid).toEqual(out.react)
  })
})
