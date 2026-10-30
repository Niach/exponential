// FEED-128: the keyboard + pointer model of Tabs, Segmented, Accordion,
// Resizable, Button, Toggle and Link in headless Chromium (React's
// browser/round1.test.ts `keyboard (a11y.json)`, round2.test.ts
// `Resizable`, the round-3 Segmented bar), run on BOTH harnesses — the
// React reference and this renderer — with the same interaction: the
// focus walk, the host's `__xuiLog`, the data model and the DOM afterwards
// (serialised, generated ids normalised) must be identical, and each
// expectation of React's suite holds on the Solid side.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { panelExtents } from "@exponential_app/eui"
import { bunHarness, launchChromium, openPage, type Query, type RunningHarness } from "../../exponential-ui/conformance/browser-page"
import { firstDomDiff, normalizeIds, type DomNode } from "../../exponential-ui/conformance/parity-compare"

let browser: Browser
let react: RunningHarness
let solid: RunningHarness
beforeAll(async () => {
  ;[react, solid] = await Promise.all([bunHarness(join(import.meta.dirname, `..`, `..`, `exponential-ui-react`, `harness`, `serve.ts`)).start(), bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start()])
  browser = await launchChromium()
})
afterAll(async () => {
  await browser?.close()
  await Promise.all([react?.stop(), solid?.stop()])
})

const tree = (node: unknown, extra: Query = {}): Query => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify({}), theme: `neutral`, mode: `light`, width: 700, ...extra })
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.textContent?.trim() ?? ``)
/** Radix moves roving focus in a timeout: press, then let it land. */
async function press(page: Page, key: string) {
  await page.keyboard.press(key)
  await page.waitForTimeout(80)
}
const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))
const changed = (name = `changed`) => ({ change: { event: { name } } })

/** The surface DOM as the parity probe serialises it. */
const dom = (page: Page, scope: string) =>
  page.evaluate((scope) => {
    const ser = (n: Node): unknown => {
      if (n.nodeType === 3) return (n as Text).data.length ? { x: (n as Text).data } : null
      if (n.nodeType !== 1) return null
      const e = n as Element
      if (e.localName === `style`) return null
      const a = Array.from(e.attributes).map((x) => [x.name, x.value]).sort((p, q) => (p[0]! < q[0]! ? -1 : p[0]! > q[0]! ? 1 : 0))
      return { t: e.localName, a, c: Array.from(e.childNodes).map(ser).filter(Boolean) }
    }
    return Array.from(document.querySelectorAll(scope)).map(ser)
  }, scope) as Promise<DomNode[]>

interface Observed {
  steps: unknown[]
  log: unknown[]
  data: unknown
  dom: DomNode[]
}

/** Run one interaction on both harnesses; the observations must match. */
async function both(query: Query, run: (page: Page, step: (v: unknown) => void) => Promise<void>, viewport = { width: 1200, height: 900 }, scope = `.xui-surface`): Promise<Observed> {
  const side = async (h: RunningHarness): Promise<Observed> => {
    const page = await openPage(browser, h.baseUrl, query, viewport)
    try {
      const steps: unknown[] = []
      await run(page, (v) => steps.push(v))
      await frame(page)
      return { steps, log: await page.evaluate(() => window.__xuiLog), data: await page.evaluate(() => window.__xuiData?.() ?? null), dom: await dom(page, scope) }
    } finally {
      await page.context().close()
    }
  }
  const [r, s] = [await side(react), await side(solid)]
  expect(s.steps).toEqual(r.steps)
  expect(s.log).toEqual(r.log)
  expect(s.data).toEqual(r.data)
  expect(firstDomDiff(normalizeIds(r.dom), normalizeIds(s.dom))).toBeUndefined()
  return s
}

declare global {
  interface Window {
    __xuiLog: unknown[]
    __xuiData?: () => unknown
  }
}

const tabsNode = { id: `t`, component: `Tabs`, props: { tabs: [{ label: `One`, value: `1` }, { label: `Two`, value: `2` }, { label: `Three`, value: `3` }] }, on: changed(), children: [1, 2, 3].map((i) => ({ id: `p${i}`, component: `Text`, props: { text: `panel ${i}` } })) }

describe(`Tabs`, () => {
  it(`ArrowRight activates the next tab and wraps; Home/End`, async () => {
    const o = await both(tree(tabsNode), async (page, step) => {
      await page.focus(`[role="tab"]`)
      await press(page, `ArrowRight`)
      step(await focused(page))
      step(await page.textContent(`[role="tab"][aria-selected="true"]`))
      step(Boolean(await page.getAttribute(`[role="tab"][aria-selected="true"]`, `id`)))
      await press(page, `End`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `ArrowLeft`)
      await press(page, `Home`)
      step(await focused(page))
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[role="tab"]`)).map((t) => t.getAttribute(`tabindex`))))
    })
    expect(o.steps).toEqual([`Two`, `Two`, true, `Three`, `One`, `One`, [`0`, `-1`, `-1`]])
  })
  it(`rtl: ArrowLeft is the next tab; ArrowUp/Down do nothing`, async () => {
    const o = await both(tree(tabsNode, { dir: `rtl` }), async (page, step) => {
      await page.focus(`[role="tab"]`)
      await press(page, `ArrowLeft`)
      step(await focused(page))
      await press(page, `ArrowDown`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      step(await focused(page))
    })
    expect(o.steps).toEqual([`Two`, `Two`, `One`])
  })
  it(`a click selects; Tab from the list enters the active tab; close + new emit`, async () => {
    const node = { ...tabsNode, props: { ...tabsNode.props, closable: `always`, addable: true }, on: { ...changed(), close: { event: { name: `closed` } }, new: { event: { name: `added` } } } }
    const o = await both(tree(node), async (page, step) => {
      await page.click(`[role="tab"] >> nth=2`)
      step(await page.textContent(`[role="tab"][aria-selected="true"]`))
      await page.click(`[aria-label="Close Two"]`)
      await page.click(`[aria-label="New tab"]`)
      await page.focus(`[role="tablist"]`)
      await page.waitForTimeout(80)
      step(await focused(page))
    })
    expect(o.steps).toEqual([`Three`, `Three`])
    expect(o.log.length).toBe(3)
  })
})

describe(`Segmented`, () => {
  it(`a roving tab stop, arrows move and wrap; Space presses`, async () => {
    const node = { id: `g`, component: `Segmented`, props: { items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }, { label: `C`, value: `c`, disabled: true }], value: `a` }, on: changed() }
    const o = await both(tree(node), async (page, step) => {
      await page.focus(`[data-xui-id="g"] button`)
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `ArrowLeft`)
      await press(page, `Space`)
      step(await page.getAttribute(`[data-xui-id="g"] [aria-checked="true"]`, `data-state`))
      step(await page.textContent(`[data-xui-id="g"] [aria-checked="true"]`))
      await press(page, `Space`)
      step(await page.textContent(`[data-xui-id="g"] [aria-checked="true"]`))
    })
    expect(o.steps).toEqual([`B`, `A`, `on`, `B`, `B`])
    expect(o.log).toEqual([{ action: `changed`, event: `change`, componentId: `g`, context: { value: `b` } }])
  })
  it(`multiple: Enter toggles items on and off (comma-joined)`, async () => {
    const node = { id: `g`, component: `Segmented`, props: { type: `multiple`, items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: `a` }, on: changed() }
    const o = await both(tree(node, { dir: `rtl` }), async (page, step) => {
      await page.focus(`[data-xui-id="g"] button`)
      await press(page, `ArrowLeft`)
      step(await focused(page))
      await press(page, `Enter`)
      await press(page, `Home`)
      await press(page, `Enter`)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[data-xui-id="g"] button`)).map((b) => b.getAttribute(`aria-pressed`))))
    })
    expect(o.steps).toEqual([`B`, [`false`, `true`]])
  })
  it(`bar: full width, tabBar tall, icon above the label, arrows rove, Enter picks`, async () => {
    const node = { id: `root`, component: `Box`, style: { display: `flex`, flexDirection: `column`, width: 390 }, children: [{ id: `tb`, component: `Segmented`, props: { variant: `bar`, items: [{ label: `Inbox`, value: `inbox`, icon: `nav-inbox` }, { label: `Issues`, value: `issues`, icon: `nav-issues` }], value: `inbox` }, on: changed() }] }
    const o = await both(tree(node), async (page, step) => {
      step(
        await page.evaluate(() => {
          const bar = document.querySelector(`[data-xui-id="tb"]`)!.getBoundingClientRect()
          const item = document.querySelector(`[data-xui-id="tb"] .xui-Segmented-item`)!
          const icon = item.querySelector(`.xui-Segmented-icon`)!.getBoundingClientRect()
          const label = item.querySelector(`.xui-Segmented-label`)!.getBoundingClientRect()
          return { w: bar.width, h: bar.height, iconAbove: icon.bottom <= label.top + 0.5, centred: Math.abs(icon.left + icon.width / 2 - (label.left + label.width / 2)) < 1, role: document.querySelector(`[data-xui-id="tb"]`)!.tagName }
        })
      )
      await page.focus(`[data-xui-id="tb"] [aria-current="page"]`)
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `Enter`)
      step(await page.textContent(`[data-xui-id="tb"] [aria-current="page"]`))
    })
    expect(o.steps).toEqual([{ w: 390, h: 56, iconAbove: true, centred: true, role: `NAV` }, `Issues`, `Issues`])
  })
})

describe(`Accordion`, () => {
  const node = (type = `single`) => ({ id: `a`, component: `Accordion`, props: { type, items: [{ title: `First`, value: `1` }, { title: `Second`, value: `2` }, { title: `Third`, value: `3` }] }, on: changed(), children: [1, 2, 3].map((i) => ({ id: `b${i}`, component: `Text`, props: { text: `body ${i}` } })) })
  it(`ArrowDown/Home/End move between headers (wrapping), Enter and Space toggle`, async () => {
    const o = await both(tree(node()), async (page, step) => {
      await page.focus(`.xui-Accordion-trigger`)
      await press(page, `ArrowDown`)
      step(await focused(page))
      await press(page, `End`)
      await press(page, `ArrowDown`)
      step(await focused(page))
      await press(page, `ArrowUp`)
      step(await focused(page))
      await press(page, `Home`)
      step(await focused(page))
      await press(page, `Enter`)
      step(await page.getAttribute(`.xui-Accordion-trigger >> nth=0`, `aria-expanded`))
      await press(page, `ArrowDown`)
      await press(page, `Space`)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`.xui-Accordion-trigger`)).map((t) => t.getAttribute(`aria-expanded`))))
      await page.keyboard.press(`Space`)
      // Radix Presence: the closing panel stays painted through its exit animation.
      step(await page.evaluate(() => document.querySelectorAll(`.xui-Accordion-content`)[1]!.hasAttribute(`hidden`)))
      await page.waitForTimeout(1000)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`.xui-Accordion-content`)).map((c) => c.hasAttribute(`hidden`))))
    })
    expect(o.steps).toEqual([`Second`, `First`, `Third`, `First`, `true`, [`false`, `true`, `false`], false, [true, true, true]])
    expect(o.log).toEqual([`1`, `2`, ``].map((value) => ({ action: `changed`, event: `change`, componentId: `a`, context: { value } })))
  })
  it(`multiple: clicks open and close independently`, async () => {
    const o = await both(tree(node(`multiple`)), async (page, step) => {
      await page.click(`.xui-Accordion-trigger >> nth=0`)
      await page.click(`.xui-Accordion-trigger >> nth=2`)
      await frame(page)
      await page.click(`.xui-Accordion-trigger >> nth=0`)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`.xui-Accordion-trigger`)).map((t) => t.getAttribute(`aria-expanded`))))
      await page.waitForTimeout(1000)
    })
    expect(o.steps).toEqual([[`false`, `false`, `true`]])
  })
})

describe(`Resizable`, () => {
  it(`panels take panelExtents of the group; a drag moves the handle with the pointer; keys step 10`, async () => {
    const o = await both(
      { view: `round2` },
      async (page, step) => {
        const box = (id: string) => page.evaluate((id) => {
          const r = document.querySelector(`[data-xui-id="${id}"]`)!.getBoundingClientRect()
          return { x: r.x, y: r.y, w: r.width, h: r.height }
        }, id)
        const group = await box(`rz`)
        const want = panelExtents([30, 70], group.w, 1)
        step(Math.abs((await box(`rz.panel.0`)).w - want[0]!) <= 1)
        step(Math.abs((await box(`rz.panel.1`)).w - want[1]!) <= 1)
        const handle = await box(`rz.handle.0`)
        await page.mouse.move(handle.x + 0.5, handle.y + 60)
        await page.mouse.down()
        await page.mouse.move(handle.x + 0.5 + (group.w - 1) * 0.2, handle.y + 60, { steps: 5 })
        await page.mouse.up()
        await frame(page)
        const size = () => page.evaluate(() => Number(document.querySelector<HTMLElement>(`[data-xui-id="rz.panel.0"]`)!.dataset.size))
        const after = await size()
        step(Math.round(after))
        await page.focus(`[data-xui-id="rz.handle.0"]`)
        await page.keyboard.press(`ArrowLeft`)
        await frame(page)
        step(Math.abs((await size()) - (after - 10)) < 1e-3)
      },
      { width: 390, height: 900 },
      // The round-2 page holds other lanes' natives: compare the Resizable.
      `[data-xui-id="rz"]`
    )
    expect(o.steps).toEqual([true, true, 50, true])
  })
  it(`rtl + vertical: keys and drags follow the direction; change {sizes} reaches the host`, async () => {
    const node = (direction: string) => ({ id: `rz`, component: `Resizable`, props: { direction, sizes: [40, 60], panels: [{ min: 20, max: 70 }, {}], handle: true }, on: changed(), style: { width: 401, height: 401 }, children: [{ id: `x`, component: `Box` }, { id: `y`, component: `Box` }] })
    const run = async (page: Page, step: (v: unknown) => void) => {
      const sizes = () => page.evaluate(() => [0, 1].map((i) => Number(document.querySelector<HTMLElement>(`[data-xui-id="rz.panel.${i}"]`)!.dataset.size)))
      await page.focus(`[data-xui-id="rz.handle.0"]`)
      for (const key of [`ArrowLeft`, `ArrowRight`, `ArrowRight`, `ArrowDown`, `End`, `Home`, `Enter`]) {
        await page.keyboard.press(key)
        await frame(page)
        step(await sizes())
      }
      const h = await page.evaluate(() => {
        const r = document.querySelector(`[data-xui-id="rz.handle.0"]`)!.getBoundingClientRect()
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
      })
      await page.mouse.move(h.x, h.y)
      await page.mouse.down()
      await page.mouse.move(h.x + 40, h.y + 40, { steps: 4 })
      step(await page.getAttribute(`[data-xui-id="rz"]`, `data-dragging`))
      step(await page.getAttribute(`[data-xui-id="rz.handle.0"]`, `data-xs`))
      await page.mouse.up()
      await frame(page)
      step(await sizes())
    }
    await both(tree(node(`horizontal`), { dir: `rtl` }), run)
    await both(tree(node(`vertical`), { dir: `rtl` }), run)
  })
})

describe(`Button, Toggle, Link`, () => {
  it(`Enter/Space press a Button, toggle a Toggle; a denied Link's click reaches the host`, async () => {
    const node = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `b`, component: `Button`, props: { label: `Go` }, on: { press: { event: { name: `go` } } } },
        { id: `t`, component: `Toggle`, props: { label: `Bold`, pressed: false }, on: changed(`toggled`) },
        // A denied href paints as text: the click still reaches the host, never navigates.
        { id: `l`, component: `Link`, props: { label: `Docs`, href: `javascript:alert(1)` }, on: { press: { event: { name: `link` } } } },
      ],
    }
    const o = await both(tree(node), async (page, step) => {
      await page.focus(`[data-xui-id="b"]`)
      await press(page, `Enter`)
      await press(page, `Space`)
      await page.focus(`[data-xui-id="t"]`)
      await press(page, `Space`)
      step(await page.getAttribute(`[data-xui-id="t"]`, `aria-pressed`))
      await press(page, `Enter`)
      step(await page.getAttribute(`[data-xui-id="t"]`, `data-state`))
      await page.click(`[data-xui-id="l"]`)
      step(page.url().includes(`view=tree`))
      step(await page.getAttribute(`[data-xui-id="l"]`, `data-denied`))
    })
    expect(o.steps).toEqual([`true`, `off`, true, ``])
    expect(o.log.map((e) => (e as { action: string }).action)).toEqual([`go`, `go`, `toggled`, `toggled`, `link`])
  })
})
