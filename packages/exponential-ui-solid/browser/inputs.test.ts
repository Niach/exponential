// FEED-128 (natives/inputs.tsx + select.tsx in headless Chromium): React's
// browser cases for these controls run on the Solid harness
// (typing.test.ts: host-owned typing at 150 ms RTT and the stale echo;
// round1.test.ts: Radio's roving tab stop, the plain Select keyboard), and
// every keyboard/pointer script below runs on BOTH harnesses, React's and
// Solid's, with the `__xuiLog` entries and the resulting control state
// compared entry for entry.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { bunHarness, launchChromium, openPage, type Query, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

let browser: Browser
let solid: RunningHarness
let react: RunningHarness

beforeAll(async () => {
  ;[solid, react] = await Promise.all([bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start(), bunHarness(join(import.meta.dirname, `..`, `..`, `exponential-ui-react`, `harness`, `serve.ts`)).start()])
  browser = await launchChromium()
})
afterAll(async () => {
  await browser?.close()
  solid?.stop()
  react?.stop()
})

const tree = (node: unknown, data: unknown = {}): Query => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700 })
const open = (h: RunningHarness, query: Query) => openPage(browser, h.baseUrl, query)
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.textContent?.trim() ?? ``)
/** Roving focus and Radix's item focus move in a timeout: press, then let
 *  it land. */
async function press(page: Page, key: string) {
  await page.keyboard.press(key)
  await page.waitForTimeout(60)
}
type Log = Record<string, unknown>[]
const logOf = (page: Page) => page.evaluate(() => window.__xuiLog as Record<string, unknown>[])

/** The same script on React's harness and on Solid's: the logs and the
 *  probed state must be identical. */
async function bothAgree(query: Query, script: (page: Page) => Promise<void>, probe: (page: Page) => Promise<unknown> = async () => null): Promise<{ log: Log; state: unknown }> {
  const run = async (h: RunningHarness) => {
    const page = await open(h, query)
    try {
      await script(page)
      await page.waitForTimeout(250)
      return { log: await logOf(page), state: await probe(page) }
    } finally {
      await page.context().close()
    }
  }
  const r = await run(react)
  const s = await run(solid)
  expect(s.log).toEqual(r.log)
  expect(s.state).toEqual(r.state)
  return r
}

declare global {
  interface Window {
    __xuiLog: unknown[]
  }
}

const EXPECTED = `abcdefghijklmnopqrstuvwxyz0123456789ABCD`

describe(`host-owned input at 150 ms RTT (typing.test.ts)`, () => {
  for (const run of [1, 2, 3]) {
    it(`run ${run}: 40 keys, none dropped, the echo settles`, async () => {
      const page = await open(solid, { view: `kitchen-sink`, echo: 150, theme: `neutral` })
      const input = page.locator(`[data-xui-id="echo-field"] input`)
      await input.focus()
      await input.pressSequentially(EXPECTED, { delay: 5 })
      await page.waitForTimeout(450)
      expect(await input.inputValue()).toBe(EXPECTED)
      await page.locator(`[data-xui-id="echo-field"] input`).blur()
      await page.waitForTimeout(400)
      expect(((await page.locator(`#host-echo`).textContent()) ?? ``).replace(/^host: /, ``)).toBe(EXPECTED)
      const log = (await logOf(page)) as { input?: string; kind?: string; revision?: number }[]
      const inputs = log.filter((e) => e.input === `title`)
      expect(inputs.length).toBeGreaterThan(0)
      expect(inputs.some((e) => e.kind === `commit`)).toBe(true)
      await page.context().close()
    })
  }
  it(`a stale host echo never clobbers a newer keystroke`, async () => {
    const page = await open(solid, { view: `kitchen-sink`, echo: 400, theme: `neutral` })
    const input = page.locator(`[data-xui-id="echo-field"] input`)
    await input.focus()
    await input.pressSequentially(`abc`, { delay: 5 })
    await page.waitForTimeout(200)
    await input.pressSequentially(`def`, { delay: 5 })
    await page.waitForTimeout(900)
    expect(await input.inputValue()).toBe(`abcdef`)
    await page.context().close()
  })
  it(`the debounced change + the commit: the same onInput entries as React`, async () => {
    const node = { id: `t`, component: `Input`, props: { label: `T`, name: `title`, value: { path: `/t` } } }
    await bothAgree(
      { ...tree(node, { t: `` }), echo: 150 },
      async (page) => {
        const input = page.locator(`input`)
        await input.focus()
        await input.pressSequentially(`hello`, { delay: 5 })
        await page.waitForTimeout(400)
        await press(page, `Enter`)
        await input.blur()
      },
      (page) => page.locator(`input`).inputValue()
    )
  })
})

describe(`keyboard models (round1.test.ts)`, () => {
  it(`Radio: a roving tab stop, arrows move and wrap`, async () => {
    const page = await open(solid, tree({ id: `r`, component: `Radio`, props: { name: `size`, label: `Size`, options: [{ label: `S`, value: `s` }, { label: `M`, value: `m` }], value: `s` } }))
    // Radix checks a radio on an ARROW-driven focus, so hold the key the way
    // a person does (a synthetic press releases before the focus lands).
    const hold = async (key: string) => {
      await page.keyboard.down(key)
      await page.waitForTimeout(80)
      await page.keyboard.up(key)
    }
    await page.focus(`[role="radio"][data-state="checked"]`)
    await hold(`ArrowDown`)
    expect(await page.getAttribute(`[role="radio"][data-state="checked"]`, `value`)).toBe(`m`)
    await hold(`ArrowDown`)
    expect(await page.getAttribute(`[role="radio"][data-state="checked"]`, `value`)).toBe(`s`)
    await page.context().close()
  })
  it(`Select (plain): ArrowDown opens on the list, Enter chooses`, async () => {
    const page = await open(solid, tree({ id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }] }, on: { change: { event: { name: `pick` } } } }))
    await page.focus(`.xui-Select-trigger`)
    await press(page, `ArrowDown`)
    await page.waitForSelector(`[role="listbox"]`)
    await press(page, `ArrowDown`)
    await press(page, `Enter`)
    await page.waitForTimeout(100)
    const log = (await logOf(page)) as { action: string; context: { value: string } }[]
    expect(log.find((l) => l.action === `pick`)?.context.value).toBe(`b`)
    await page.context().close()
  })
})

const on = (name: string) => ({ change: { event: { name } } })

describe(`interaction parity with React (the same script, the same __xuiLog)`, () => {
  it(`Radio: roving keys, Home/End, a click`, async () => {
    const node = { id: `r`, component: `Radio`, props: { name: `size`, label: `Size`, options: [{ label: `S`, value: `s` }, { label: `M`, value: `m` }, { label: `L`, value: `l`, disabled: true }, { label: `XL`, value: `xl` }], value: `s` }, on: on(`size`) }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`[role="radio"][data-state="checked"]`)
        for (const key of [`ArrowDown`, `ArrowDown`, `End`, `ArrowUp`]) {
          await page.keyboard.down(key)
          await page.waitForTimeout(80)
          await page.keyboard.up(key)
        }
        await page.click(`text=S`)
      },
      async (page) => ({ checked: await page.getAttribute(`[role="radio"][data-state="checked"]`, `value`), focus: await focused(page), tabStops: await page.$$eval(`[role="radio"]`, (els) => els.map((e) => e.getAttribute(`tabindex`))) })
    )
  })
  it(`Select (plain): open by key, typeahead, arrows, Enter; Escape closes and refocuses the trigger`, async () => {
    const node = { id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }, { label: `Bravo`, value: `br`, disabled: true }, { label: `Gamma`, value: `g` }], value: `a` }, on: on(`pick`) }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`.xui-Select-trigger`)
        // Closed type-ahead on the trigger picks directly.
        await press(page, `b`)
        await page.waitForTimeout(1100)
        await press(page, ` `)
        await page.waitForSelector(`[role="listbox"]`)
        await page.waitForTimeout(100)
        await press(page, `ArrowDown`)
        await press(page, `End`)
        await press(page, `ArrowUp`)
        await press(page, `Escape`)
        await page.waitForTimeout(100)
        await press(page, `Enter`)
        await page.waitForSelector(`[role="listbox"]`)
        await page.waitForTimeout(100)
        await press(page, `ArrowDown`)
        await press(page, `Enter`)
      },
      async (page) => ({ trigger: await page.textContent(`.xui-Select-trigger`), expanded: await page.getAttribute(`.xui-Select-trigger`, `aria-expanded`), focus: await page.evaluate(() => document.activeElement?.className), open: await page.$$eval(`[role="listbox"]`, (l) => l.length) })
    )
  })
  it(`Select (plain): pointer open, hover highlights, click picks`, async () => {
    const node = { id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }] }, on: on(`pick`) }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.click(`.xui-Select-trigger`)
        await page.waitForSelector(`[role="option"]`)
        await page.waitForTimeout(150)
        await page.hover(`[role="option"] >> text=Beta`)
        await page.click(`[role="option"] >> text=Beta`)
      },
      async (page) => ({ trigger: await page.textContent(`.xui-Select-trigger`), open: await page.$$eval(`[role="listbox"]`, (l) => l.length) })
    )
  })
  it(`Select (searchable): typing filters and fires a debounced search, Enter picks the active option`, async () => {
    const node = { id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, searchable: true, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }, { label: `Bet`, value: `bt` }] }, on: { change: { event: { name: `pick` } }, search: { event: { name: `q` } } } }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.click(`.xui-Select-trigger`)
        await page.waitForSelector(`.xui-Select-search`)
        await page.keyboard.type(`be`, { delay: 20 })
        await page.waitForTimeout(300)
        await press(page, `ArrowDown`)
        await press(page, `Enter`)
      },
      async (page) => ({ trigger: await page.textContent(`.xui-Select-trigger`), open: await page.$$eval(`[role="listbox"]`, (l) => l.length), focus: await page.evaluate(() => document.activeElement?.className) })
    )
  })
  it(`Select (multiple): ArrowDown opens, Enter toggles, typeahead moves the active option, Escape closes`, async () => {
    const node = { id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, multiple: true, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }, { label: `Gamma`, value: `g` }], value: [] }, on: on(`pick`) }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`.xui-Select-trigger`)
        await press(page, `ArrowDown`)
        await page.waitForSelector(`[role="listbox"]`)
        await page.waitForTimeout(100)
        await press(page, `Enter`)
        await press(page, `g`)
        await press(page, ` `)
        await press(page, `Home`)
        await press(page, `ArrowUp`)
        await press(page, `Enter`)
        await press(page, `Escape`)
      },
      async (page) => ({ trigger: await page.textContent(`.xui-Select-trigger`), open: await page.$$eval(`[role="listbox"]`, (l) => l.length), focus: await page.evaluate(() => document.activeElement?.className) })
    )
  })
  it(`Slider: arrows, Shift+arrow, PageUp/PageDown, Home/End; a click on the track`, async () => {
    const node = { id: `sl`, component: `Slider`, props: { label: `Volume`, min: 0, max: 100, step: 5, value: 40 }, on: on(`vol`) }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`[role="slider"]`)
        for (const key of [`ArrowRight`, `ArrowUp`, `Shift+ArrowRight`, `ArrowLeft`, `PageDown`, `Home`, `End`, `PageUp`]) await press(page, key)
        const box = (await page.locator(`.xui-slider-root`).boundingBox())!
        await page.mouse.click(box.x + box.width * 0.25, box.y + box.height / 2)
      },
      async (page) => ({ now: await page.getAttribute(`[role="slider"]`, `aria-valuenow`), wrapper: await page.$eval(`[role="slider"]`, (e) => (e.parentElement as HTMLElement).getAttribute(`style`)), value: await page.textContent(`.xui-Slider-value`) })
    )
  })
  it(`NumberField: steppers, arrows, Page keys, Home/End, typing, Enter`, async () => {
    const node = { id: `n`, component: `NumberField`, props: { label: `Seats`, name: `seats`, min: 1, max: 50, value: 3, unit: `seats` }, on: { change: { event: { name: `n` } }, submit: { event: { name: `go` } } } }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.click(`[data-xui-part="NumberField/increment"]`)
        await page.click(`[data-xui-part="NumberField/increment"]`)
        await page.click(`[data-xui-part="NumberField/decrement"]`)
        await page.focus(`input`)
        for (const key of [`ArrowUp`, `Shift+ArrowUp`, `PageUp`, `ArrowDown`, `PageDown`, `End`, `Home`]) await press(page, key)
        await page.fill(`input`, `77`)
        await press(page, `Enter`)
        await page.locator(`input`).blur()
      },
      async (page) => ({ value: await page.locator(`input`).inputValue(), now: await page.getAttribute(`input`, `aria-valuenow`), text: await page.getAttribute(`input`, `aria-valuetext`), inc: await page.getAttribute(`[data-xui-part="NumberField/increment"]`, `disabled`) })
    )
  })
  it(`Checkbox: Space toggles, Enter does not; Switch: Space and the row toggle`, async () => {
    const node = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `c`, component: `Checkbox`, props: { label: `Agree`, name: `agree`, checked: false }, on: on(`agree`) },
        { id: `w`, component: `Switch`, props: { label: `Auto`, name: `auto`, description: `Hourly`, checked: false }, on: on(`auto`) },
      ],
    }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`[role="checkbox"]`)
        await press(page, ` `)
        await press(page, `Enter`)
        await press(page, ` `)
        await press(page, ` `)
        await page.focus(`[role="switch"]`)
        await press(page, ` `)
        await page.click(`.xui-Switch-description`)
        await page.click(`.xui-Switch-label`)
      },
      async (page) => ({ checkbox: await page.getAttribute(`[role="checkbox"]`, `aria-checked`), switch: await page.getAttribute(`[role="switch"]`, `aria-checked`) })
    )
  })
  it(`Composer: typing, Shift+Enter, Enter submits and clears`, async () => {
    const node = { id: `co`, component: `Composer`, props: { placeholder: `Ask` }, on: { submit: { event: { name: `send` } }, change: { event: { name: `typed` } } } }
    await bothAgree(
      tree(node),
      async (page) => {
        await page.focus(`textarea`)
        await page.keyboard.type(`hi`, { delay: 10 })
        await page.keyboard.press(`Shift+Enter`)
        await page.keyboard.type(`there`, { delay: 10 })
        await page.waitForTimeout(300)
        await press(page, `Enter`)
      },
      async (page) => ({ value: await page.locator(`textarea`).inputValue(), disabled: await page.getAttribute(`.xui-Composer-send`, `disabled`) })
    )
  })
})
