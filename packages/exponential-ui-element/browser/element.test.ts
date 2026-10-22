// FEED-130: the element API in headless Chromium, through the harness's
// `?view=mount` (`__xuiMount` paints a tree in a fresh
// `<exponential-surface>`) and then the element itself in-page: the host is
// `display: contents` (the `.xui-surface` root is the box), an attribute
// change repaints, a press dispatches a bubbling `xui-action`, `remove()`
// disposes the painted surface.

import { afterAll, beforeAll, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

let served: RunningHarness
let browser: Browser
let page: Page

const tree = {
  id: `root`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: 8, padding: 16 },
  children: [
    { id: `title`, component: `Text`, props: { text: `Hello` } },
    { id: `go`, component: `Button`, props: { label: `Go` }, on: { press: { event: { name: `go` } } } },
  ],
}

beforeAll(async () => {
  served = await bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start()
  browser = await launchChromium()
  page = await openPage(browser, served.baseUrl, { view: `mount` }, { width: 800, height: 600 }, { waitForSurface: false })
  await page.waitForFunction(() => typeof (window as unknown as { __xuiMount?: unknown }).__xuiMount === `function`)
  await page.evaluate((t) => (window as unknown as { __xuiMount: (t: unknown) => Promise<void> }).__xuiMount(t), tree)
})

afterAll(async () => {
  await browser?.close()
  await served?.stop()
})

it(`the host element is display: contents around one .xui-surface`, async () => {
  const got = await page.evaluate(() => {
    const el = document.querySelector(`exponential-surface`)!
    const surface = el.querySelector(`.xui-surface`)
    return { display: getComputedStyle(el).display, surfaces: el.querySelectorAll(`.xui-surface`).length, shadow: el.shadowRoot !== null, child: surface?.parentElement === el, id: surface?.getAttribute(`data-xui-surface`) }
  })
  expect(got).toEqual({ display: `contents`, surfaces: 1, shadow: false, child: true, id: `m` })
})

it(`an attribute change repaints`, async () => {
  const theme = () => page.evaluate(() => document.querySelector(`exponential-surface .xui-surface`)!.getAttribute(`data-xui-theme`))
  expect(await theme()).toBe(`neutral`)
  await page.evaluate(() => document.querySelector(`exponential-surface`)!.setAttribute(`theme`, `orange`))
  await page.waitForFunction(() => document.querySelector(`exponential-surface .xui-surface`)?.getAttribute(`data-xui-theme`) === `orange`)
  expect(await theme()).toBe(`orange`)
  // `width` as an attribute: a numeric string is px.
  await page.evaluate(() => document.querySelector(`exponential-surface`)!.setAttribute(`width`, `320`))
  await page.waitForFunction(() => document.querySelector<HTMLElement>(`exponential-surface .xui-surface`)!.getBoundingClientRect().width === 320)
})

it(`a press dispatches a bubbling xui-action`, async () => {
  await page.evaluate(() => {
    ;(window as unknown as { __events: unknown[] }).__events = []
    document.addEventListener(`xui-action`, (e) => {
      const d = (e as CustomEvent<{ name: string; componentId: string; waitUntil?: unknown }>).detail
      ;(window as unknown as { __events: unknown[] }).__events.push({ name: d.name, componentId: d.componentId, waitUntil: typeof d.waitUntil, target: (e.target as Element).tagName.toLowerCase() })
    })
  })
  await page.click(`[data-xui-id="go"]`)
  await page.waitForFunction(() => (window as unknown as { __events: unknown[] }).__events.length > 0)
  const events = await page.evaluate(() => (window as unknown as { __events: unknown[] }).__events)
  expect(events).toEqual([{ name: `go`, componentId: `go`, waitUntil: `function`, target: `exponential-surface` }])
})

it(`remove() disposes the painted surface`, async () => {
  const got = await page.evaluate(async () => {
    const el = document.querySelector(`exponential-surface`)!
    const handleBefore = el.handle !== null
    el.remove()
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
    return { handleBefore, painted: el.querySelectorAll(`[data-xui-id]`).length, inPage: document.querySelectorAll(`.xui-surface`).length, handleAfter: el.handle }
  })
  expect(got).toEqual({ handleBefore: true, painted: 0, inPage: 0, handleAfter: null })
})
