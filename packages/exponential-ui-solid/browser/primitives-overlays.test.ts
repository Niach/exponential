// FEED-131: the overlay primitives' interaction model in headless Chromium,
// on BOTH harnesses (`?view=primitives&id=<specimen>`): the same interaction
// on the React reference (Radix) and the Solid twin, then the observations
// and the whole <body> (serialised, generated ids normalised) must be
// identical, and each expectation of Radix's behaviour holds.

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
const count = (page: Page, selector: string) => page.locator(selector).count()
const attr = (page: Page, selector: string, name: string) => page.locator(selector).first().getAttribute(name)
const center = async (page: Page, selector: string) => {
  const box = (await page.locator(selector).first().boundingBox())!
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 }
}

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
async function both(id: string, run: (page: Page, step: (v: unknown) => void) => Promise<void>, viewport = { width: 1100, height: 700 }, compareDom = true): Promise<unknown[]> {
  const side = async (h: RunningHarness) => {
    const page = await openPage(browser, h.baseUrl, { view: `primitives`, id, theme: `exponential`, mode: `light`, width: viewport.width }, viewport, { waitForSurface: false })
    try {
      await page.waitForSelector(`[data-xui-specimen]`, { state: `attached` })
      await frame(page)
      const steps: unknown[] = []
      await run(page, (v) => steps.push(v))
      await frame(page)
      return { steps, dom: compareDom ? await body(page) : [] }
    } finally {
      await page.context().close()
    }
  }
  const [r, s] = [await side(react), await side(solid)]
  expect(s.steps).toEqual(r.steps)
  if (compareDom) expect(firstDomDiff(normalizeIds(r.dom), normalizeIds(s.dom))).toBeUndefined()
  return s.steps
}

const trigger = (label: string) => `[data-xui-specimen] button:text-is("${label}")`

describe(`Dialog / Sheet / AlertDialog`, () => {
  it(`the trigger opens, focus moves inside, Escape closes, focus returns`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Dialog`)).click()
      await page.waitForTimeout(100)
      step(await count(page, `[role="dialog"]`))
      step(await page.evaluate(() => Boolean(document.activeElement?.closest(`[role="dialog"]`))))
      step(await focused(page))
      step(await attr(page, `body`, `data-scroll-locked`))
      await press(page, `Tab`)
      step(await focused(page))
      await press(page, `Escape`)
      await page.waitForTimeout(300)
      step(await count(page, `[role="dialog"]`))
      step(await focused(page))
      step(await attr(page, `body`, `style`))
    })
    expect(steps[0]).toBe(1)
    expect(steps[1]).toBe(true)
    expect(steps[2]).toBe(`Cancel`)
    expect(steps[3]).toBe(`1`)
    expect(steps[5]).toBe(0)
    expect(steps[6]).toBe(`Dialog`)
  })

  it(`a pointer down outside (the overlay) closes the dialog; the alert dialog ignores it`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Dialog`)).click()
      await page.waitForTimeout(100)
      await page.mouse.click(10, 690)
      await page.waitForTimeout(300)
      step(await count(page, `[role="dialog"]`))
      await page.locator(trigger(`Alert`)).click()
      await page.waitForTimeout(100)
      step(await count(page, `[role="alertdialog"]`))
      step(await focused(page))
      await page.mouse.click(10, 690)
      await page.waitForTimeout(300)
      step(await count(page, `[role="alertdialog"]`))
      await press(page, `Escape`)
      await page.waitForTimeout(300)
      step(await count(page, `[role="alertdialog"]`))
      step(await focused(page))
    })
    // This alert has no Cancel to take the focus: it stays on the trigger.
    expect(steps).toEqual([0, 1, `Alert`, 1, 0, `Alert`])
  })

  it(`the phone sheet drags down to dismiss; a short drag springs back`, async () => {
    const viewport = { width: 390, height: 800 }
    const steps = await both(
      `overlays/dialog-sheet-phone`,
      async (page, step) => {
        await page.waitForTimeout(300)
        step(await attr(page, `[data-slot="dialog-content"]`, `data-side`))
        const grab = await center(page, `[data-slot="sheet-grabber"]`)
        await page.mouse.move(grab.x, grab.y)
        await page.mouse.down()
        await page.mouse.move(grab.x, grab.y + 20, { steps: 4 })
        await page.waitForTimeout(400)
        await page.mouse.up()
        await page.waitForTimeout(400)
        step(await count(page, `[role="dialog"]`))
        await page.mouse.move(grab.x, grab.y)
        await page.mouse.down()
        await page.mouse.move(grab.x, grab.y + 160, { steps: 8 })
        await page.mouse.up()
        await page.waitForTimeout(800)
        step(await count(page, `[role="dialog"]`))
        step(await focused(page))
      },
      viewport
    )
    expect(steps).toEqual([`bottom`, 1, 0, `Edit profile`])
  })
})

describe(`DropdownMenu`, () => {
  it(`opens on press, arrows rove, Escape closes back to the trigger`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Menu`)).click()
      await page.waitForTimeout(100)
      step(await count(page, `[role="menu"]`))
      step(await attr(page, `body`, `style`))
      await press(page, `ArrowDown`)
      step(await focused(page))
      step(await attr(page, `[role="menuitem"]`, `data-highlighted`))
      await press(page, `Escape`)
      await page.waitForTimeout(200)
      step(await count(page, `[role="menu"]`))
      step(await focused(page))
    })
    expect(steps).toEqual([1, `pointer-events: none;`, `Item`, ``, 0, `Menu`])
  })

  it(`typeahead, a checkbox item toggles and keeps the menu open, a radio item picks`, async () => {
    const steps = await both(`overlays/dropdown-menu`, async (page, step) => {
      await page.waitForTimeout(150)
      await press(page, `ArrowDown`)
      step(await focused(page))
      await press(page, `b`)
      step(await focused(page))
      await page.locator(`[role="menuitemcheckbox"]:text-is("Activity bar")`).click()
      await page.waitForTimeout(100)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[role="menuitemcheckbox"]`)).map((el) => el.getAttribute(`aria-checked`))))
      step(await count(page, `[role="menu"]`))
      await page.locator(`[role="menuitemradio"]:text-is("Top")`).click()
      await page.waitForTimeout(200)
      step(await count(page, `[role="menu"]`))
      await page.locator(`[data-xui-specimen] button`).first().click()
      await page.waitForTimeout(150)
      step(await page.evaluate(() => Array.from(document.querySelectorAll(`[role="menuitemradio"]`)).map((el) => el.getAttribute(`aria-checked`))))
    })
    expect(steps[0]).toBe(`Profile⇧⌘P`)
    expect(steps[1]).toBe(`Billing`)
    expect(steps[2]).toEqual([`true`, `true`])
    expect(steps[3]).toBe(1)
    expect(steps[4]).toBe(0)
    // The items' state lives inside the content: a reopened menu starts over.
    expect(steps[5]).toEqual([`false`, `true`, `false`])
  })

  it(`ArrowRight opens a sub menu and focuses it, ArrowLeft returns`, async () => {
    const steps = await both(`overlays/dropdown-menu-sub`, async (page, step) => {
      await page.waitForTimeout(150)
      await press(page, `End`)
      step(await focused(page))
      await press(page, `ArrowRight`)
      await page.waitForTimeout(150)
      step(await focused(page))
      step(await attr(page, `[role="menuitem"]:text-is("Danger zone")`, `aria-expanded`))
      await press(page, `ArrowLeft`)
      await page.waitForTimeout(150)
      step(await focused(page))
      step(await attr(page, `[role="menuitem"]:text-is("Danger zone")`, `aria-expanded`))
      // The popper measures its anchor while the trigger's highlight transition runs.
    }, undefined, false)
    expect(steps).toEqual([`Danger zone`, `Delete team`, `true`, `Danger zone`, `false`])
  })
})

describe(`Tooltip / Popover / HoverCard`, () => {
  it(`a tooltip opens on hover after the delay and closes on leave`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Tooltip`)).hover()
      await page.waitForTimeout(200)
      step(await count(page, `[role="tooltip"]`))
      step(await attr(page, trigger(`Tooltip`), `data-state`))
      step(await page.evaluate(() => document.querySelector(`[role="tooltip"]`)?.textContent))
      // Out of the grace area (the move that leaves the trigger is never judged).
      await page.mouse.move(1000, 380, { steps: 5 })
      await page.waitForTimeout(300)
      step(await count(page, `[role="tooltip"]`))
    })
    expect(steps).toEqual([1, `delayed-open`, `Shown on hover`, 0])
  })

  it(`a popover closes on an outside click, focus stays where it landed`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Popover`)).click()
      await page.waitForTimeout(150)
      step(await count(page, `[data-slot="popover-content"]`))
      step(await page.evaluate(() => document.activeElement?.getAttribute(`data-slot`)))
      await page.mouse.click(1000, 380)
      await page.waitForTimeout(300)
      step(await count(page, `[data-slot="popover-content"]`))
      await page.locator(trigger(`Popover`)).click()
      await page.waitForTimeout(150)
      await press(page, `Escape`)
      await page.waitForTimeout(300)
      step(await count(page, `[data-slot="popover-content"]`))
      step(await focused(page))
    })
    expect(steps).toEqual([1, `popover-content`, 0, 0, `Popover`])
  })

  it(`a hover card opens after openDelay, the pointer can travel into it, it closes after leaving`, async () => {
    const steps = await both(`overlays/triggers`, async (page, step) => {
      await page.locator(trigger(`Hover card`)).hover()
      await page.waitForTimeout(150)
      step(await count(page, `[data-slot="hover-card-content"]`))
      await page.waitForTimeout(400)
      step(await count(page, `[data-slot="hover-card-content"]`))
      const card = await center(page, `[data-slot="hover-card-content"]`)
      await page.mouse.move(card.x, card.y, { steps: 3 })
      await page.waitForTimeout(300)
      step(await count(page, `[data-slot="hover-card-content"]`))
      await page.mouse.move(1000, 380)
      await page.waitForTimeout(400)
      step(await count(page, `[data-slot="hover-card-content"]`))
    })
    expect(steps).toEqual([0, 1, 1, 0])
  })
})

describe(`Toaster`, () => {
  it(`a toast auto-dismisses after its duration; hover expands the stack and pauses it`, async () => {
    const steps = await both(`overlays/toast-live`, async (page, step) => {
      await page.locator(trigger(`Notify`)).click()
      await page.waitForTimeout(100)
      step(await count(page, `[data-slot="toast"]`))
      await page.waitForTimeout(800)
      step(await count(page, `[data-slot="toast"]`))
      await page.locator(trigger(`Notify`)).click()
      await page.waitForTimeout(50)
      await page.locator(`[data-slot="toast"]`).hover()
      await page.waitForTimeout(900)
      step(await count(page, `[data-slot="toast"]`))
      step(await attr(page, `[data-slot="toast-list"]`, `data-expanded`))
      await page.mouse.move(10, 10)
      await page.waitForTimeout(900)
      step(await count(page, `[data-slot="toast"]`))
      await page.locator(trigger(`Notify`)).click()
      await page.waitForTimeout(50)
      await page.locator(`[data-slot="toast-close"]`).click()
      await page.waitForTimeout(50)
      step(await count(page, `[data-slot="toast"]`))
    })
    expect(steps).toEqual([1, 0, 1, `true`, 0, 0])
  })
})
