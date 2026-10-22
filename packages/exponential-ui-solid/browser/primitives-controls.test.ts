// FEED-131: the `controls` primitives' keyboard + pointer model in headless
// Chromium, on BOTH harnesses (`?view=primitives&id=<specimen>`): the same
// interaction on the React reference and the Solid twin, then the focus
// walk, the observations and the whole <body> (serialised, generated ids
// normalised) must be identical.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"
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

const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))
/** Radix moves focus in timeouts: press, then let it land. */
async function press(page: Page, key: string) {
  await page.keyboard.press(key)
  await page.waitForTimeout(80)
}
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.textContent?.trim() ?? ``)
const attr = (page: Page, selector: string, name: string) => page.evaluate(([s, n]) => document.querySelector(s!)?.getAttribute(n!) ?? null, [selector, name])

/** The whole <body> as the parity probe serialises it (styles, scripts dropped). */
const body = (page: Page) =>
  page.evaluate(() => {
    const ser = (n: Node): unknown => {
      if (n.nodeType === 3) return (n as Text).data.length ? { x: (n as Text).data } : null
      if (n.nodeType !== 1) return null
      const e = n as Element
      if (e.localName === `style` || e.localName === `script`) return null
      const a = Array.from(e.attributes).map((x) => [x.name, x.value]).sort((p, q) => (p[0]! < q[0]! ? -1 : p[0]! > q[0]! ? 1 : 0))
      return { t: e.localName, a, c: Array.from(e.childNodes).map(ser).filter(Boolean) }
    }
    return [ser(document.body)]
  }) as Promise<DomNode[]>

/** One specimen, one interaction, on both harnesses: the observations and
 *  the <body> afterwards must match. Returns Solid's observations. */
async function both(id: string, run: (page: Page, step: (v: unknown) => void) => Promise<void>, viewport = { width: 900, height: 700 }): Promise<unknown[]> {
  const side = async (h: RunningHarness) => {
    const page = await openPage(browser, h.baseUrl, { view: `primitives`, id, theme: `exponential`, mode: `light`, width: viewport.width }, viewport, { waitForSurface: false })
    try {
      await page.waitForSelector(`[data-xui-specimen]`, { state: `attached` })
      await frame(page)
      const steps: unknown[] = []
      await run(page, (v) => steps.push(v))
      await frame(page)
      return { steps, dom: await body(page) }
    } finally {
      await page.context().close()
    }
  }
  const [r, s] = [await side(react), await side(solid)]
  expect(s.steps).toEqual(r.steps)
  expect(firstDomDiff(normalizeIds(r.dom), normalizeIds(s.dom))).toBeUndefined()
  return s.steps
}

// ── checkbox · switch · tabs · collapsible · scroll-area ──

const ariaChecked = (page: Page, role: string) => page.evaluate((r) => Array.from(document.querySelectorAll(`[role="${r}"]`)).map((el) => el.getAttribute(`aria-checked`)), role)

describe(`Checkbox / Switch`, () => {
  it(`click and Space toggle, Enter never does; disabled stays`, async () => {
    const steps = await both(`controls/checkbox`, async (page, step) => {
      const boxes = page.locator(`[data-slot="checkbox"]`)
      await boxes.nth(0).click()
      step(await ariaChecked(page, `checkbox`))
      await boxes.nth(2).click()
      step(await ariaChecked(page, `checkbox`))
      await boxes.nth(1).focus()
      await press(page, `Enter`)
      step(await ariaChecked(page, `checkbox`))
      await press(page, ` `)
      step(await ariaChecked(page, `checkbox`))
      await boxes.nth(3).click({ force: true })
      step(await ariaChecked(page, `checkbox`))
      // The form one: its bubble input follows.
      await boxes.nth(6).click()
      step(await page.evaluate(() => (document.querySelector(`form input[type="checkbox"]`) as HTMLInputElement).checked))
    })
    expect(steps[0]).toEqual([`true`, `true`, `mixed`, `false`, `true`, `false`, `true`])
    // The indeterminate one is controlled (no onCheckedChange): it stays mixed.
    expect(steps[1]).toEqual(steps[0])
    expect(steps[2]).toEqual(steps[1])
    expect(steps[3]).toEqual([`true`, `false`, `mixed`, `false`, `true`, `false`, `true`])
    expect(steps[5]).toBe(false)
  })

  it(`switches toggle by click and keyboard`, async () => {
    const steps = await both(`controls/switch`, async (page, step) => {
      const switches = page.locator(`[data-slot="switch"]`)
      await switches.nth(0).click()
      await switches.nth(4).click()
      step(await ariaChecked(page, `switch`))
      await switches.nth(1).focus()
      await press(page, ` `)
      await press(page, `Enter`)
      step(await ariaChecked(page, `switch`))
    })
    expect(steps[0]).toEqual([`true`, `true`, `false`, `false`, `false`, `true`])
    expect(steps[1]).toEqual([`true`, `true`, `false`, `false`, `false`, `true`])
  })
})

describe(`Tabs`, () => {
  it(`Tab enters on the active trigger, arrows rove (skipping disabled, looping) and select`, async () => {
    const steps = await both(`controls/tabs`, async (page, step) => {
      await page.mouse.click(5, 5)
      await press(page, `Tab`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      step(await focused(page))
      await press(page, `End`)
      step(await focused(page))
      await press(page, `Home`)
      step(await focused(page))
      step(await page.evaluate(() => document.querySelector(`[role="tabpanel"]:not([hidden])`)?.textContent))
      await page.locator(`[role="tab"]`).nth(1).click()
      step(await page.evaluate(() => document.querySelector(`[role="tabpanel"]:not([hidden])`)?.textContent))
    })
    expect(steps).toEqual([`Password`, `Account`, `Password`, `Password`, `Account`, `Account settings`, `Change your password`])
  })

  it(`the segmented control's segments are tabs`, async () => {
    const steps = await both(`controls/segmented-control`, async (page, step) => {
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[role="tab"]`)).map((t) => `${t.textContent}:${t.getAttribute(`data-state`)}`)))
    })
    expect(steps[0]).toEqual([`List:active`, `Board:inactive`, `Off:inactive`, `Inbox:inactive`, `My issues:active`])
  })
})

describe(`Collapsible`, () => {
  it(`the trigger toggles the content (measured size variables follow)`, async () => {
    const steps = await both(`controls/collapsible`, async (page, step) => {
      const triggers = page.locator(`[data-slot="collapsible-trigger"]`)
      await triggers.nth(1).click()
      await frame(page)
      step(await attr(page, `[data-slot="collapsible"]:nth-of-type(2) [data-slot="collapsible-content"]`, `data-state`))
      await triggers.nth(0).click()
      await frame(page)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[data-slot="collapsible-content"]`)).map((c) => (c as HTMLElement).hidden)))
      await triggers.nth(0).focus()
      await press(page, `Enter`)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[data-slot="collapsible-content"]`)).map((c) => (c as HTMLElement).hidden)))
    })
    expect(steps).toEqual([`open`, [true, false], [false, false]])
  })
})

describe(`ScrollArea`, () => {
  it(`type="hover": the scrollbar appears while the pointer is over the area and hides after the delay`, async () => {
    const bar = (page: Page) => page.evaluate(() => Array.from(document.querySelectorAll(`[data-slot="scroll-area-scrollbar"]`)).map((b) => b.getAttribute(`data-state`)))
    const steps = await both(`controls/scroll-area`, async (page, step) => {
      step(await bar(page))
      const hover = page.locator(`[data-slot="scroll-area"]`).nth(1)
      await hover.hover()
      await page.waitForFunction(() => document.querySelectorAll(`[data-slot="scroll-area-thumb"]`).length === 2)
      await page.waitForTimeout(100)
      step(await bar(page))
      step(await page.evaluate(() => (document.querySelectorAll(`[data-slot="scroll-area-thumb"]`)[1] as HTMLElement).getAttribute(`style`)))
      await page.evaluate(() => (document.querySelectorAll(`[data-slot="scroll-area-viewport"]`)[1] as HTMLElement).scrollTo(0, 60))
      await page.waitForTimeout(150)
      step(await page.evaluate(() => (document.querySelectorAll(`[data-slot="scroll-area-thumb"]`)[1] as HTMLElement).style.transform))
      await page.mouse.move(800, 280)
      await page.waitForTimeout(900)
      step(await bar(page))
    })
    // The `always` area's bar carries no data-state (Radix's visible bar).
    expect(steps[0]).toEqual([null])
    expect(steps[1]).toEqual([null, `visible`])
    expect(steps[3]).not.toBe(`translate3d(0px, 0px, 0px)`)
    expect(steps[4]).toEqual([null])
  })
})

// ── select ──

describe(`Select`, () => {
  const trigger = (page: Page, n = 0) => page.locator(`[data-slot="select-trigger"]`).nth(n)
  const listbox = (page: Page) => page.evaluate(() => Boolean(document.querySelector(`[role="listbox"]`)))
  const value = (page: Page, n = 0) => page.evaluate((n) => document.querySelectorAll(`[data-slot="select-value"]`)[n]?.textContent ?? null, n)

  it(`click opens, ArrowDown + Enter picks, the trigger shows it`, async () => {
    const steps = await both(`controls/select-closed`, async (page, step) => {
      await trigger(page).click()
      await page.waitForTimeout(80)
      step({ open: await listbox(page), focused: await focused(page) })
      await press(page, `ArrowDown`)
      step({ focused: await focused(page) })
      await press(page, `Enter`)
      await page.waitForTimeout(80)
      step({ open: await listbox(page), value: await value(page), focused: await page.evaluate(() => document.activeElement?.getAttribute(`data-slot`) ?? null) })
    })
    expect(steps).toEqual([{ open: true, focused: `Apple` }, { focused: `Banana` }, { open: false, value: `Banana`, focused: `select-trigger` }])
  })

  it(`type-ahead on the closed trigger, then Space opens on the chosen item and Escape closes`, async () => {
    const steps = await both(`controls/select-closed`, async (page, step) => {
      await trigger(page).focus()
      await press(page, `c`)
      step({ open: await listbox(page), value: await value(page) })
      await page.waitForTimeout(1100)
      await press(page, `Enter`)
      await page.waitForTimeout(80)
      step({ open: await listbox(page), focused: await focused(page) })
      await press(page, `ArrowDown`)
      step({ focused: await focused(page) })
      await press(page, `Escape`)
      step({ open: await listbox(page), value: await value(page) })
    })
    expect(steps).toEqual([{ open: false, value: `Carrot` }, { open: true, focused: `Carrot` }, { focused: `Carrot` }, { open: false, value: `Carrot` }])
  })

  for (const id of [`controls/select-open`, `controls/select-open-popper`]) {
    it(`${id}: arrows, type-ahead inside the list, Enter`, async () => {
      const steps = await both(id, async (page, step) => {
        await page.waitForTimeout(80)
        step({ open: await listbox(page), focused: await focused(page) })
        await press(page, `ArrowUp`)
        step({ focused: await focused(page) })
        await press(page, `End`)
        step({ focused: await focused(page) })
        await press(page, `a`)
        step({ focused: await focused(page) })
        await press(page, `Enter`)
        await page.waitForTimeout(80)
        step({ open: await listbox(page), value: await value(page) })
      })
      expect(steps[0]).toEqual({ open: true, focused: id === `controls/select-open` ? `Banana` : `Carrot` })
      expect(steps[steps.length - 1]).toEqual({ open: false, value: `Apple` })
    })
  }
})

// ── command ──

describe(`Command`, () => {
  const sel = `[data-slot="command-item"][aria-selected="true"]`
  const state = (page: Page) => page.evaluate((sel) => ({ query: (document.querySelector(`[data-slot="command-input"]`) as HTMLInputElement).value, selected: document.querySelector(sel)?.getAttribute(`data-value`) ?? null, items: Array.from(document.querySelectorAll(`[data-slot="command-item"]`)).map((el) => el.getAttribute(`data-value`)), empty: document.querySelector(`[data-slot="command-empty"]`) !== null }), sel)
  it(`typing filters and reorders, arrows walk (skipping disabled), Enter selects`, async () => {
    const steps = await both(`controls/command`, async (page, step) => {
      step(await state(page))
      await page.click(`[data-slot="command-input"]`)
      await page.keyboard.press(`End`)
      for (let i = 0; i < 3; i++) await press(page, `Backspace`)
      step(await state(page))
      await page.keyboard.type(`a`)
      await frame(page)
      step(await state(page))
      for (const k of [`ArrowDown`, `ArrowDown`, `ArrowDown`, `ArrowUp`, `End`, `Home`]) {
        await press(page, k)
        step([k, await state(page)])
      }
      await press(page, `ArrowDown`)
      await press(page, `Enter`)
      step(await state(page))
      await page.keyboard.type(`zzz`)
      await frame(page)
      step(await state(page))
      await page.click(`[data-slot="command-input-clear"]`)
      await frame(page)
      step([await state(page), await focused(page), await attr(page, `[data-slot="command-input"]`, `aria-activedescendant`) !== null])
    })
    expect(steps[0]).toMatchObject({ query: `app`, selected: `Apple`, items: [`Apple`, `Pineapple`, `Apple pie tool`] })
    expect(steps[1]).toMatchObject({ query: ``, selected: `Banana`, items: [`Banana`, `Apple`, `Pineapple`, `Hammer`, `Apple pie tool`] })
    expect((steps[3] as [string, { selected: string }])[1].selected).not.toBe(`Pineapple`)
  })
})
