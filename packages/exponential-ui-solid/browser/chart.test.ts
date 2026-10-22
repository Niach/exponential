// FEED-128 (the Chart + ChoiceTiles lane): interaction parity in headless
// Chromium. Every scripted interaction runs on the React harness (the
// reference) AND the Solid one; the `__xuiLog` entries and the DOM the test
// reads back (the node's subtree serialised with sorted attributes) must be
// IDENTICAL. Chart: pointer moves across the plot (band, dimmed bars, the
// bigger dot, the tooltip), a pie slice's pointerenter, keys on the focused
// plot (arrows, rtl, Home/End/Escape), pointer leave + blur clear.
// ChoiceTiles: fixtures/interactions-round1.json `choiceTiles` (a press on the
// tile, its label or its picture), arrows single/multiple (rtl, wrap, a
// disabled tile skipped), Space/Enter, Tab to the one stop.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import { readFileSync } from "node:fs"
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

declare global {
  interface Window {
    __xuiLog: unknown[]
  }
}

const treeView = (node: unknown, data: unknown = {}, dir: `ltr` | `rtl` = `ltr`): Query => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700, dir }) as Query
const settle = (page: Page, ms = 100) => page.waitForTimeout(ms)
const log = (page: Page) => page.evaluate(() => window.__xuiLog) as Promise<unknown[]>
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.getAttribute(`data-value`) ?? document.activeElement?.localName ?? null)
/** The node's subtree, attributes sorted, generated id values masked. */
const dom = (page: Page, id: string) =>
  page.evaluate((id) => {
    const walk = (n: Node): string => {
      if (n.nodeType === 3) return n.nodeValue ?? ``
      if (n.nodeType !== 1) return ``
      const e = n as Element
      const attrs = Array.from(e.attributes)
        .map((a) => [a.name, a.name === `id` ? `#` : a.value])
        .sort(([a], [b]) => (a < b ? -1 : 1))
        .map(([k, v]) => ` ${k}="${v}"`)
        .join(``)
      return `<${e.localName}${attrs}>${Array.from(e.childNodes).map(walk).join(``)}</${e.localName}>`
    }
    const el = document.querySelector(`[data-xui-id="${id}"]`)
    return el ? walk(el) : null
  }, id)

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

const bars = { id: `c`, component: `Chart`, props: { kind: `bar`, showValues: true, categories: [`Mon`, `Tue`, `Wed`], series: [{ name: `Ok`, values: [3, 5, 2] }, { name: `Failed`, values: [1, 4, 2], tone: `danger` }] } }
const line = { id: `c`, component: `Chart`, props: { kind: `line`, categories: [`a`, `b`, `c`, `d`], series: [{ name: `x`, values: [3, 1, 7, 2] }] } }
const sparkline = { id: `c`, component: `Chart`, props: { kind: `sparkline`, height: 40, series: [{ name: `s`, values: [4, 8, 1, 9, 3] }] } }
const donut = { id: `c`, component: `Chart`, props: { kind: `donut`, categories: [`a`, `b`, `c`], series: [{ name: `in`, values: [1, 2, 3] }, { name: `out`, values: [3, 2, 1] }] } }

/** Pointer moves across the plot, left to right, then out. */
async function sweep(page: Page) {
  const seen: unknown[] = []
  const box = (await page.locator(`[data-xui-id="c"] svg`).boundingBox())!
  for (const f of [0.02, 0.2, 0.5, 0.8, 0.98]) {
    await page.mouse.move(box.x + box.width * f, box.y + box.height / 2)
    await settle(page, 50)
    seen.push(await dom(page, `c`))
  }
  await page.mouse.move(box.x + box.width / 2, box.y + box.height + 200)
  await settle(page, 50)
  seen.push(await dom(page, `c`))
  return seen
}

describe(`Chart`, () => {
  for (const [name, node, dir] of [[`bar`, bars, `ltr`], [`bar rtl`, bars, `rtl`], [`line`, line, `ltr`], [`sparkline`, sparkline, `ltr`]] as const)
    it(`hover sweep: ${name}`, async () => {
      const out = await both(treeView(node, {}, dir), async (page) => ({ seen: await sweep(page), log: await log(page) }))
      // A sparkline has no tooltip (no pointer tracking, no keys).
      expect(out.react.seen.some((s) => String(s).includes(`Chart/tooltip`))).toBe(name !== `sparkline`)
      expect(out.solid).toEqual(out.react)
    })

  it(`donut: a slice's pointerenter highlights its category; leaving clears`, async () => {
    const out = await both(treeView(donut), async (page) => {
      const seen: unknown[] = []
      for (const i of [0, 2, 1]) {
        const path = page.locator(`[data-xui-id="c"] path[data-category="${i}"]`).last()
        const b = (await path.boundingBox())!
        // A point inside the slice: its ring's middle along the box diagonal
        // is not always inside, so hover through Playwright's own pick.
        await path.hover({ position: { x: b.width / 2, y: b.height / 2 }, force: true })
        await settle(page, 50)
        seen.push(await dom(page, `c`))
      }
      await page.mouse.move(5, 590)
      await settle(page, 50)
      seen.push(await dom(page, `c`))
      return { seen, log: await log(page) }
    })
    expect(out.solid).toEqual(out.react)
  })

  for (const [name, node, dir] of [[`bar`, bars, `ltr`], [`bar rtl`, bars, `rtl`], [`donut`, donut, `ltr`]] as const)
    it(`keys on the focused plot: ${name}`, async () => {
      const out = await both(treeView(node, {}, dir), async (page) => {
        const seen: unknown[] = []
        await page.keyboard.press(`Tab`)
        seen.push(await focused(page))
        for (const k of [`ArrowRight`, `ArrowRight`, `ArrowLeft`, `ArrowLeft`, `ArrowLeft`, `End`, `Home`, `Escape`, `ArrowLeft`]) {
          await page.keyboard.press(k)
          await settle(page, 30)
          seen.push(await dom(page, `c`))
        }
        await page.keyboard.press(`Tab`)
        await settle(page, 30)
        seen.push(await focused(page), await dom(page, `c`))
        return { seen, log: await log(page) }
      })
      expect(out.react.seen[0]).toBe(`svg`)
      expect(out.solid).toEqual(out.react)
    })
})

interface ChoiceCase {
  name: string
  type: `single` | `multiple`
  value: string | null
  disabled?: number
  presses: number[]
  expect: { value: string; changes: string[] }
}
const choiceCases = (JSON.parse(readFileSync(join(packages, `exponential-ui`, `fixtures`, `interactions-round1.json`), `utf8`)) as { choiceTiles: ChoiceCase[] }).choiceTiles
// A picture that loads offline (the harness fails a page on a resource error).
const PICTURE = `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="16" height="10"><rect width="16" height="10" fill="#3a7"/></svg>`)}`
const choiceNode = (type: string, disabled?: number, pictures = true) => ({
  id: `t`,
  component: `ChoiceTiles`,
  props: { name: `t`, type, options: [`a`, `b`, `c`, `d`].map((v, i) => ({ label: v.toUpperCase(), value: v, ...(pictures ? { image: PICTURE } : {}), ...(disabled === i ? { disabled: true } : {}) })), value: { path: `/v` }, columns: 2 },
  on: { change: { event: { name: `change`, context: { v: { path: `/v` } } } } },
})
const picked = (page: Page) => page.evaluate(() => Array.from(document.querySelectorAll(`.xui-ChoiceTiles-tile[aria-checked="true"]`)).map((b) => b.getAttribute(`data-value`)).join(`,`))

describe(`ChoiceTiles`, () => {
  for (const c of choiceCases)
    it(`fixtures/interactions-round1.json: ${c.name}`, async () => {
      const parts = [`tile`, `label`, `picture`]
      const out = await both(treeView(choiceNode(c.type, c.disabled), c.value === null ? {} : { v: c.value }), async (page) => {
        for (const [k, i] of c.presses.entries()) {
          await page.click(`[data-xui-id="t.${parts[k % parts.length]}.${i}"]`, { force: true })
          await settle(page, 50)
        }
        return { picked: await picked(page), focused: await focused(page), dom: await dom(page, `t`), log: await log(page) }
      })
      expect(out.react.picked).toBe(c.expect.value)
      expect(out.solid).toEqual(out.react)
    })

  for (const [name, type, dir, disabled, keys] of [
    [`single: arrows pick with wrap, a disabled tile skipped`, `single`, `ltr`, 2, [`ArrowRight`, `ArrowRight`, `ArrowDown`, `ArrowLeft`, `ArrowUp`, `ArrowUp`]],
    [`single rtl: ArrowLeft moves forward`, `single`, `rtl`, undefined, [`ArrowLeft`, `ArrowLeft`, `ArrowRight`]],
    [`multiple: arrows move focus, Space/Enter toggle`, `multiple`, `ltr`, 1, [`ArrowRight`, `Space`, `ArrowRight`, `Enter`, `ArrowLeft`, `Space`, `Space`]],
  ] as const)
    it(`keys: ${name}`, async () => {
      const out = await both(treeView(choiceNode(type, disabled, false), { v: `a` }, dir), async (page) => {
        const seen: unknown[] = []
        await page.keyboard.press(`Tab`)
        seen.push(await focused(page))
        for (const k of keys) {
          await page.keyboard.press(k)
          await settle(page, 30)
          seen.push(await focused(page), await picked(page))
        }
        seen.push(await dom(page, `t`))
        return { seen, log: await log(page) }
      })
      expect(out.react.seen[0]).toBe(`a`)
      expect(out.solid).toEqual(out.react)
    })
})
