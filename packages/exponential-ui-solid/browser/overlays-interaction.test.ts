// FEED-128: the overlays' interaction model in headless Chromium (React's
// browser/round1.test.ts overlay cases, hardening.test.ts's hover card,
// round4's `submitClosesOverlay`, plus dismissal, focus and the toast
// queue), run on BOTH harnesses — the React reference and this renderer —
// with the same interaction: the observations, the host's `__xuiLog`, the
// data model and the DOM afterwards (serialised, generated ids normalised)
// must be identical, and each expectation of React's suite holds.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import round4 from "@exponential-at/ui/fixtures/round4-contract.json"
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

declare global {
  interface Window {
    __xuiLog: unknown[]
    __xuiData?: () => unknown
  }
}

const tree = (node: unknown, data: unknown = {}, extra: Query = {}): Query => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700, ...extra })
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.textContent?.trim() ?? ``)
async function press(page: Page, key: string) {
  await page.keyboard.press(key)
  await page.waitForTimeout(80)
}
const settle = (page: Page, ms = 400) => page.waitForTimeout(ms)
const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))
const count = (page: Page, sel: string) => page.locator(sel).count()

const dom = (page: Page) =>
  page.evaluate(() => {
    const ser = (n: Node): unknown => {
      if (n.nodeType === 3) return (n as Text).data.length ? { x: (n as Text).data } : null
      if (n.nodeType !== 1) return null
      const e = n as Element
      if (e.localName === `style`) return null
      const a = Array.from(e.attributes).map((x) => [x.name, x.value]).sort((p, q) => (p[0]! < q[0]! ? -1 : p[0]! > q[0]! ? 1 : 0))
      return { t: e.localName, a, c: Array.from(e.childNodes).map(ser).filter(Boolean) }
    }
    return Array.from(document.querySelectorAll(`.xui-surface`)).map(ser)
  }) as Promise<DomNode[]>

interface Observed {
  steps: unknown[]
  log: unknown[]
  data: unknown
}

/** Run one interaction on both harnesses; the observations (and, unless
 *  `compareDom: false`, the DOM afterwards) must match. */
async function both(query: Query, run: (page: Page, step: (v: unknown) => void) => Promise<void>, opts: { viewport?: { width: number; height: number }; compareDom?: boolean } = {}): Promise<Observed> {
  const side = async (h: RunningHarness) => {
    const page = await openPage(browser, h.baseUrl, query, opts.viewport ?? { width: 1200, height: 900 })
    try {
      const steps: unknown[] = []
      await run(page, (v) => steps.push(v))
      await frame(page)
      return { steps, log: await page.evaluate(() => window.__xuiLog), data: await page.evaluate(() => window.__xuiData?.() ?? null), dom: await dom(page) }
    } finally {
      await page.context().close()
    }
  }
  const [r, s] = [await side(react), await side(solid)]
  expect(s.steps).toEqual(r.steps)
  expect(s.log).toEqual(r.log)
  expect(s.data).toEqual(r.data)
  if (opts.compareDom !== false) expect(firstDomDiff(normalizeIds(r.dom), normalizeIds(s.dom))).toBeUndefined()
  return s
}

// A Button's click never reaches a Dialog/Popover trigger wrapper on either
// renderer (the Button stops its click; Radix's Dialog/Popover triggers
// open on click): those triggers are Text here.
const text = (id: string, t: string) => ({ id, component: `Text`, props: { text: t } })
const button = (id: string, label: string, extra: Record<string, unknown> = {}) => ({ id, component: `Button`, props: { label, ...extra } })

describe(`audit bug C: breakpoints match inside overlays`, () => {
  it(`a node in an open Dialog wears its @media (min-width) block, in the container's overlay layer`, async () => {
    const node = { id: `d`, component: `Dialog`, props: { title: `Hi`, open: true }, children: [{ id: `inner`, component: `Text`, props: { text: `wide` }, style: { "@media (min-width: 600px)": { fontWeight: 700 } } }] }
    const o = await both({ ...tree(node), width: 800 }, async (page, step) => {
      await page.waitForSelector(`[data-xui-id="inner"]`, { state: `attached` })
      step(await page.evaluate(() => getComputedStyle(document.querySelector(`[data-xui-id="inner"]`)!).fontWeight))
      step(await page.evaluate(() => Boolean(document.querySelector(`[data-xui-id="inner"]`)!.closest(`.xui-container [data-xui-layer="overlay"]`))))
    })
    expect(o.steps).toEqual([`700`, true])
  })
})

describe(`Dialog + Drawer`, () => {
  const dialog = (props: Record<string, unknown> = {}) => ({
    id: `root`,
    component: `Box`,
    children: [{ id: `d`, component: `Dialog`, props: { title: `Edit`, description: `Change it`, open: { path: `/open` }, ...props }, on: { change: { event: { name: `changed` } } }, slots: { trigger: text(`d-t`, `Open`) }, children: [button(`d-a`, `First`), button(`d-b`, `Second`)] }],
  })
  it(`the trigger opens it (bound open written, change fired), focus lands on the first tabbable, Tab loops inside, Escape closes`, async () => {
    const o = await both(tree(dialog(), { open: false }), async (page, step) => {
      await page.click(`[data-xui-id="d-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Dialog"]`)
      await settle(page, 200)
      step(await focused(page))
      await press(page, `Tab`)
      step(await focused(page))
      await press(page, `Tab`)
      step(await focused(page))
      await press(page, `Tab`)
      step(await focused(page))
      step(await page.evaluate(() => document.body.style.pointerEvents))
      step(await page.evaluate(() => document.querySelector(`[data-xui-id="root"]`)!.getAttribute(`aria-hidden`)))
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Dialog"]`))
      step(await page.evaluate(() => document.body.style.pointerEvents))
    })
    expect(o.steps).toEqual([`First`, `Second`, ``, `First`, `none`, `true`, 0, ``])
    expect(o.data).toEqual({ open: false })
    expect(o.log.map((l) => (l as { context: unknown }).context)).toEqual([{ open: true }, { open: false }])
  })
  it(`a press on the backdrop closes a dismissible Dialog; the close button too; a non-dismissible one stays`, async () => {
    const o = await both(tree(dialog(), { open: true }), async (page, step) => {
      await page.waitForSelector(`[data-xui-overlay="Dialog"]`)
      await settle(page, 200)
      await page.mouse.click(5, 5)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Dialog"]`))
      await page.click(`[data-xui-id="d-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Dialog"]`)
      await settle(page, 200)
      await page.click(`.xui-Dialog-close`)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Dialog"]`))
    })
    expect(o.steps).toEqual([0, 0])
    const pinned = await both(tree(dialog({ dismissible: false }), { open: true }), async (page, step) => {
      await page.waitForSelector(`[data-xui-overlay="Dialog"]`)
      await settle(page, 200)
      await page.mouse.click(5, 5)
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Dialog"]`))
      step(await page.getAttribute(`[data-xui-overlay="Dialog"]`, `role`))
      step(await count(page, `.xui-Dialog-close`))
    })
    expect(pinned.steps).toEqual([1, `alertdialog`, 0])
  })
  it(`a Drawer drags to dismiss past 80 px and springs back short of it`, async () => {
    const node = { id: `dr`, component: `Drawer`, props: { title: `Filters`, side: `bottom`, open: true }, children: [text(`dr-c`, `Body`)] }
    const o = await both(tree(node), async (page, step) => {
      await page.waitForSelector(`.xui-Drawer-handle`)
      await settle(page, 600)
      const box = (await page.locator(`.xui-Drawer-handle`).boundingBox())!
      const x = box.x + box.width / 2
      const y = box.y + box.height / 2
      await page.mouse.move(x, y)
      await page.mouse.down()
      await page.mouse.move(x, y + 30, { steps: 5 })
      await page.waitForTimeout(400)
      await page.mouse.up()
      await settle(page)
      step(await count(page, `[data-xui-overlay="Drawer"]`))
      await page.mouse.move(x, y)
      await page.mouse.down()
      await page.mouse.move(x, y + 120, { steps: 5 })
      await page.mouse.up()
      await settle(page, 700)
      step(await count(page, `[data-xui-overlay="Drawer"]`))
    })
    expect(o.steps).toEqual([1, 0])
  })
})

describe(`round 4: a valid Form submit closes the nearest dismissible Dialog or Drawer`, () => {
  for (const c of round4.closeOnSubmit) {
    it(c.name, async () => {
      const o = await both(tree(c.input, c.data), async (page, step) => {
        await settle(page, 300)
        const go = `[data-xui-id="${c.form}-go"]`
        // A Popover open at mount next to an opening Dialog loses to its
        // focus (Radix: focus outside dismisses): open it again, as a person would.
        if ((await count(page, go)) === 0) for (const t of await page.locator(`[data-xui-id$="-trigger"]`).all()) await t.click()
        await settle(page, 300)
        // A programmatic click (React's round4.test.tsx `fireEvent.click`):
        // a Popover inside a Dialog sits under the Dialog's backdrop on both.
        await page.$eval(go, (el) => (el as HTMLElement).click())
        await settle(page)
        step(await count(page, `[data-xui-id="${c.form}"]`))
      })
      expect(o.log.map((l) => (l as { action: string }).action)).toEqual(c.actions)
      expect(o.steps).toEqual([c.expected.overlay ? 0 : 1])
      expect(o.data).toEqual(c.expected.data)
    })
  }
})

describe(`Popover + Tooltip`, () => {
  it(`a press opens a Popover, a press outside closes it, Escape closes it`, async () => {
    const node = { id: `root`, component: `Box`, style: { padding: 80 }, children: [{ id: `p`, component: `Popover`, props: { side: `bottom` }, slots: { trigger: text(`p-t`, `Open`) }, children: [button(`p-in`, `Inside`)] }] }
    const o = await both(tree(node), async (page, step) => {
      await page.click(`[data-xui-id="p-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await settle(page, 200)
      step(await focused(page))
      step(await page.getAttribute(`[data-xui-overlay="Popover"]`, `data-side`))
      await page.mouse.click(600, 600)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Popover"]`))
      await page.click(`[data-xui-id="p-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await settle(page, 200)
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[data-xui-overlay="Popover"]`))
    })
    expect(o.steps).toEqual([`Inside`, `bottom`, 0, 0])
  })
  it(`Tooltip shows on keyboard focus; Popover openOn hover opens on hover and on focus`, async () => {
    const node = {
      id: `root`,
      component: `Box`,
      style: { display: `flex`, gap: 40, padding: 80 },
      children: [
        { id: `tip`, component: `Tooltip`, props: { content: `Hint text` }, children: [text(`tip-t`, `Hover me`)] },
        { id: `hc`, component: `Popover`, props: { openOn: `hover` }, slots: { trigger: button(`hc-t`, `Card`) }, children: [text(`hc-c`, `Card body`)] },
      ],
    }
    const o = await both(
      tree(node),
      async (page, step) => {
        await press(page, `Tab`)
        await page.waitForSelector(`[data-xui-overlay="Tooltip"]`, { timeout: 3000 })
        step(await page.getAttribute(`[data-xui-overlay="Tooltip"]`, `data-state`))
        step(await page.evaluate(() => document.querySelector(`[role="tooltip"]`)!.id === document.querySelector(`.xui-Tooltip-anchor`)!.getAttribute(`aria-describedby`)))
        await press(page, `Escape`)
        await settle(page)
        step(await count(page, `[data-xui-overlay="Tooltip"]`))
        await page.hover(`[data-xui-id="hc-t"]`)
        await page.waitForSelector(`[data-xui-overlay="Popover"][data-open-on="hover"]`, { timeout: 3000 })
        await page.mouse.move(1, 1)
        await page.waitForSelector(`[data-xui-overlay="Popover"]`, { state: `detached`, timeout: 3000 })
        await page.focus(`[data-xui-id="hc-t"]`)
        await page.waitForSelector(`[data-xui-overlay="Popover"]`, { timeout: 3000 })
        step(await count(page, `[data-xui-overlay="Popover"]`))
      },
      { compareDom: false }
    )
    expect(o.steps).toEqual([`instant-open`, true, 0, 1])
  })
  it(`a Tooltip shows after the hover delay and hides when the pointer leaves`, async () => {
    const node = { id: `root`, component: `Box`, style: { padding: 120 }, children: [{ id: `tip`, component: `Tooltip`, props: { content: `Hint text`, side: `bottom` }, children: [text(`tip-t`, `Hover me`)] }] }
    const o = await both(tree(node), async (page, step) => {
      await page.hover(`[data-xui-id="tip-t"]`)
      await page.waitForTimeout(100)
      step(await count(page, `[data-xui-overlay="Tooltip"]`))
      await page.waitForSelector(`[data-xui-overlay="Tooltip"]`, { timeout: 3000 })
      await settle(page, 300)
      step(await page.getAttribute(`[data-xui-overlay="Tooltip"]`, `data-state`))
      step(await page.getAttribute(`[data-xui-overlay="Tooltip"]`, `data-side`))
      await page.mouse.move(600, 600, { steps: 6 })
      await settle(page)
      step(await count(page, `[data-xui-overlay="Tooltip"]`))
    })
    expect(o.steps).toEqual([0, `delayed-open`, `bottom`, 0])
  })
  it(`an open Tooltip, a hover card and a Dialog opened by its trigger paint the same DOM`, async () => {
    const node = {
      id: `root`,
      component: `Box`,
      style: { display: `flex`, gap: 60, padding: 120 },
      children: [
        { id: `tip`, component: `Tooltip`, props: { content: `Hint text`, side: `right` }, children: [text(`tip-t`, `Hover me`)] },
        { id: `hc`, component: `Popover`, props: { openOn: `hover`, side: `top` }, slots: { trigger: text(`hc-t`, `Card`) }, children: [text(`hc-c`, `Card body`)] },
        { id: `d`, component: `Dialog`, props: { title: `Edit` }, slots: { trigger: text(`d-t`, `Open`), footer: button(`d-f`, `Save`) }, children: [text(`d-c`, `Body`)] },
      ],
    }
    await both(tree(node), async (page) => {
      await page.hover(`[data-xui-id="tip-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Tooltip"]`)
      await settle(page, 500)
    })
    await both(tree(node), async (page) => {
      await page.hover(`[data-xui-id="hc-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await settle(page, 500)
    })
    await both(tree(node), async (page) => {
      await page.click(`[data-xui-id="d-t"]`)
      await page.waitForSelector(`[data-xui-overlay="Dialog"]`)
      await settle(page, 500)
    })
  })
  it(`a hover Popover's focus is scoped to its OWN content (with A open, focus from B's trigger into B's content keeps B open)`, async () => {
    const card = (k: string) => ({ id: `hc${k}`, component: `Popover`, props: { openOn: `hover` }, slots: { trigger: button(`t${k}`, `Card ${k}`) }, children: [button(`b${k}`, `Inside ${k}`)] })
    const node = { id: `root`, component: `Box`, style: { display: `flex`, gap: 200, padding: 80 }, children: [card(`A`), card(`B`)] }
    const o = await both(
      { ...tree(node), width: 900 },
      async (page, step) => {
        await page.hover(`[data-xui-id="tA"]`)
        await page.waitForSelector(`[data-xui-overlay="Popover"]:has([data-xui-id="bA"])`, { timeout: 3000 })
        await page.hover(`[data-xui-id="bA"]`)
        await page.focus(`[data-xui-id="tB"]`)
        await page.waitForSelector(`[data-xui-overlay="Popover"]:has([data-xui-id="bB"])`, { timeout: 3000 })
        step(await count(page, `[data-xui-overlay="Popover"]`))
        await page.focus(`[data-xui-id="bB"]`)
        await settle(page, 500)
        step(await count(page, `[data-xui-overlay="Popover"]:has([data-xui-id="bB"])`))
      },
      { viewport: { width: 1000, height: 600 }, compareDom: false }
    )
    expect(o.steps).toEqual([2, 1])
  })
})

describe(`Menu`, () => {
  const menu = { id: `m`, component: `Menu`, props: { label: `View`, items: [{ label: `Copy`, value: `copy` }, { kind: `checkbox`, label: `Done`, value: `done`, checked: { path: `/done` } }, { kind: `submenu`, label: `More`, items: [{ label: `Deep`, value: `deep` }] }] }, on: { select: { event: { name: `sel` } } } }
  it(`(press) Enter opens, arrows move, a checkbox item toggles and stays open, ArrowRight opens a submenu, ArrowLeft returns`, async () => {
    const o = await both(tree(menu, { done: false }), async (page, step) => {
      await page.focus(`.xui-Menu-trigger`)
      await press(page, `Enter`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 200)
      step(await focused(page))
      await press(page, `ArrowDown`)
      step(await focused(page))
      await press(page, `Enter`)
      step(await page.evaluate(() => (window.__xuiData!() as { done: boolean }).done))
      step(await count(page, `[role="menu"]`))
      await press(page, `ArrowDown`)
      await press(page, `ArrowRight`)
      await page.waitForSelector(`[data-xui-overlay="Menu.sub"]`)
      await settle(page, 200)
      step(await focused(page))
      await press(page, `ArrowLeft`)
      await settle(page)
      step(await focused(page))
      step(await count(page, `[data-xui-overlay="Menu.sub"]`))
    })
    expect(o.steps).toEqual([`Copy`, `Done`, true, 1, `Deep`, `More`, 0])
  })
  it(`(press) a press opens it, a press on an item selects and closes it, the focus returns to the trigger; Escape closes too`, async () => {
    const o = await both(tree(menu, { done: false }), async (page, step) => {
      await page.click(`.xui-Menu-trigger`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 200)
      step(await page.evaluate(() => document.activeElement?.getAttribute(`role`)))
      step(await page.getAttribute(`.xui-Menu-trigger`, `aria-expanded`))
      await page.click(`[role="menuitem"]:has-text("Copy")`)
      await settle(page)
      step(await count(page, `[role="menu"]`))
      step(await page.evaluate(() => document.activeElement?.classList.contains(`xui-Menu-trigger`)))
      await press(page, `ArrowDown`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 200)
      step(await focused(page))
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[role="menu"]`))
      step(await page.evaluate(() => document.activeElement?.classList.contains(`xui-Menu-trigger`)))
    })
    expect(o.steps).toEqual([`menu`, `true`, 0, true, `Copy`, 0, true])
    expect(o.log).toEqual([{ action: `sel`, event: `select`, componentId: `m`, context: { value: `copy` } }])
  })
  it(`(press) the pointer highlights rows, typeahead jumps, hovering a submenu row opens it`, async () => {
    const o = await both(
      tree(menu, { done: false }),
      async (page, step) => {
        await page.click(`.xui-Menu-trigger`)
        await page.waitForSelector(`[role="menu"]`)
        await settle(page, 200)
        await page.hover(`[role="menuitemcheckbox"]`)
        step(await page.evaluate(() => document.querySelector(`[data-highlighted]`)?.textContent))
        await press(page, `c`)
        await settle(page, 100)
        step(await focused(page))
        await page.hover(`[role="menuitem"]:has-text("More")`)
        await page.waitForSelector(`[data-xui-overlay="Menu.sub"]`, { timeout: 3000 })
        await settle(page, 200)
        step(await page.getAttribute(`[role="menuitem"]:has-text("More")`, `aria-expanded`))
        await page.click(`[data-xui-overlay="Menu.sub"] [role="menuitem"]`)
        await settle(page)
        step(await count(page, `[role="menu"]`))
      },
      { compareDom: false }
    )
    expect(o.steps).toEqual([`Done`, `Copy`, `true`, 0])
    expect(o.log).toEqual([{ action: `sel`, event: `select`, componentId: `m`, context: { value: `deep` } }])
  })
  const rich = {
    id: `root`,
    component: `Box`,
    style: { padding: 40 },
    children: [
      {
        id: `m`,
        component: `Menu`,
        props: {
          label: `More`,
          icon: `star`,
          items: [
            { label: `Rename`, value: `rename`, icon: `star`, shortcut: `⌘R` },
            { kind: `checkbox`, label: `Pinned`, value: `pin`, checked: true, shortcut: `P` },
            { kind: `checkbox`, label: `Muted`, value: `mute` },
            { separator: true },
            { kind: `label`, label: `Danger` },
            { label: `Disabled`, disabled: true },
            { label: `Delete`, value: `delete`, destructive: true },
            { kind: `submenu`, label: `Move to`, icon: `star`, items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] },
            { kind: `submenu`, label: `Assign`, searchable: true, placeholder: `Find`, items: [{ label: `Alice`, value: `alice` }, { label: `Bob`, value: `bob` }] },
          ],
        },
        on: { select: { event: { name: `sel` } } },
      },
    ],
  }
  it(`an open menu (every row kind) paints the same DOM; ArrowDown highlights the first row`, async () => {
    const o = await both(tree(rich), async (page, step) => {
      await page.click(`[data-xui-id="m.trigger"]`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 300)
      await press(page, `ArrowDown`)
      await settle(page, 200)
      step(await focused(page))
    })
    expect(o.steps).toEqual([`Rename⌘R`])
  })
  it(`a submenu opened by hover paints the same DOM`, async () => {
    await both(tree(rich), async (page) => {
      await page.click(`[data-xui-id="m.trigger"]`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 300)
      await page.hover(`[role="menuitem"]:has-text("Move to")`)
      await page.waitForSelector(`[data-xui-overlay="Menu.sub"]`)
      await settle(page, 400)
    })
  })
  it(`a searchable submenu: its field takes the focus, typing filters, Enter picks and closes`, async () => {
    const o = await both(tree(rich), async (page, step) => {
      await page.click(`[data-xui-id="m.trigger"]`)
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 300)
      await page.hover(`[role="menuitem"]:has-text("Assign")`)
      await page.waitForSelector(`[data-xui-overlay="Menu.sub"][data-searchable="true"]`)
      await settle(page, 400)
      step(await page.evaluate(() => document.activeElement?.getAttribute(`placeholder`)))
      await page.keyboard.type(`bo`)
      await settle(page, 200)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[data-xui-overlay="Menu.sub"] [role="option"]`)).map((o) => o.textContent)))
      await press(page, `Enter`)
      await settle(page)
      step(await count(page, `[role="menu"]`))
    })
    expect(o.steps).toEqual([`Find`, [`Bob`], 0])
    expect(o.log).toEqual([{ action: `sel`, event: `select`, componentId: `m`, context: { value: `bob` } }])
  })
  it(`(contextmenu) Shift+F10 opens at the child's corner, a right click at the pointer; Escape closes`, async () => {
    const node = { id: `cm`, component: `Menu`, props: { openOn: `contextmenu`, items: [{ label: `Rename`, value: `rename` }] }, children: [{ id: `area`, component: `Box`, props: { pressable: true }, style: { width: 120, height: 40 } }] }
    const o = await both(tree(node), async (page, step) => {
      await page.focus(`[data-xui-id="area"]`)
      await press(page, `Shift+F10`)
      await page.waitForSelector(`[data-xui-overlay="Menu"][data-open-on="contextmenu"]`, { timeout: 3000 })
      await settle(page, 200)
      step(await focused(page))
      step(await page.getAttribute(`[data-xui-id="cm"]`, `data-state`))
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[role="menu"]`))
      await page.click(`[data-xui-id="area"]`, { button: `right`, position: { x: 30, y: 20 } })
      await page.waitForSelector(`[role="menu"]`)
      await settle(page, 200)
      const at = await page.evaluate(() => {
        const r = document.querySelector(`[role="menu"]`)!.getBoundingClientRect()
        const a = document.querySelector(`[data-xui-id="area"]`)!.getBoundingClientRect()
        return { dx: Math.round(r.left - a.left), dy: Math.round(r.top - a.top) }
      })
      step(at)
    })
    expect(o.steps).toEqual([`Rename`, `open`, 0, { dx: 32, dy: 20 }])
  })
})

describe(`Toast`, () => {
  it(`sits in the toast layer above an open Dialog, bottom-centre on phones, bottom-end when wide`, async () => {
    const node = { id: `root`, component: `Box`, children: [{ id: `d`, component: `Dialog`, props: { title: `Modal`, open: true }, children: [] }, { id: `t`, component: `Toast`, props: { title: `Saved`, duration: 0 } }] }
    for (const [vw, edge] of [
      [390, `centre`],
      [1200, `end`],
    ] as const) {
      const o = await both(
        { ...tree(node), width: vw },
        async (page, step) => {
          await page.waitForSelector(`[data-xui-id="t"]`, { state: `attached` })
          await settle(page, 300)
          step(
            await page.evaluate(() => {
              const t = document.querySelector(`[data-xui-id="t"]`)!.getBoundingClientRect()
              const top = document.elementFromPoint(t.left + t.width / 2, t.top + t.height / 2)
              return { left: Math.round(t.left), right: Math.round(t.right), top: Boolean(top?.closest(`[data-xui-id="t"]`)), vw: innerWidth }
            })
          )
        },
        { viewport: { width: vw, height: 700 } }
      )
      const r = o.steps[0] as { left: number; right: number; top: boolean; vw: number }
      expect(r.top).toBe(true)
      if (edge === `centre`) expect(Math.abs(r.left - (r.vw - r.right))).toBeLessThanOrEqual(2)
      else expect(r.vw - r.right).toBeLessThan(r.left)
    }
  })
  it(`the queue: at most 3 visible, newest nearest the edge; the duration runs (paused while hovered), then dismiss + change; Escape and the close button dismiss`, async () => {
    const toast = (i: number, extra: Record<string, unknown> = {}) => ({ id: `t${i}`, component: `Toast`, props: { title: `Toast ${i}`, duration: 0, ...extra }, on: { dismiss: { event: { name: `dismissed${i}` } } } })
    const node = { id: `root`, component: `Box`, children: [toast(1), toast(2), toast(3), toast(4, { duration: 600, type: `error` }), toast(5)] }
    const o = await both(tree(node), async (page, step) => {
      await settle(page, 200)
      step(await page.evaluate(() => Array.from(document.querySelectorAll<HTMLElement>(`[data-xui-overlay="Toast"]`)).map((t) => [t.dataset.xuiId, getComputedStyle(t).display !== `none`, t.getAttribute(`role`)])))
      await page.hover(`[data-xui-id="t4"]`)
      await page.waitForTimeout(900)
      step(await count(page, `[data-xui-id="t4"]`))
      await page.mouse.move(1, 1)
      await page.waitForTimeout(1200)
      step(await count(page, `[data-xui-id="t4"]`))
      await page.click(`[data-xui-id="t5"] .xui-Toast-close`)
      await settle(page)
      step(await count(page, `[data-xui-id="t5"]`))
      await page.focus(`[data-xui-id="t3"] .xui-Toast-close`)
      await press(page, `Escape`)
      await settle(page)
      step(await count(page, `[data-xui-id="t3"]`))
    })
    expect(o.steps).toEqual([
      [
        [`t1`, false, `status`],
        [`t2`, false, `status`],
        [`t3`, true, `status`],
        [`t4`, true, `alert`],
        [`t5`, true, `status`],
      ],
      1,
      0,
      0,
      0,
    ])
    expect(o.log.map((l) => (l as { action: string }).action)).toEqual([`dismissed4`, `dismissed5`, `dismissed3`])
  })
})
