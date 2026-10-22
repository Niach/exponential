// FEED-128: interaction parity of the date, code and media natives in
// headless Chromium: the SAME script runs against the React harness (the
// reference) and the Solid one, and every observation — the host log
// (`__xuiLog`), the focused element, the painted state — must be equal.
// Mirrors React's round1/hardening/round2/safe-host cases: the calendar's
// grid keys by the locale week (de-DE: Monday first) and by the SURFACE
// direction, the range picker's two steps, the time list's listbox keys,
// Escape / outside dismissal and the focus return, CodeBlock copy (the
// `ansi` block copies without escapes, `copied` announced), the Carousel
// keys/buttons, and the media policy (a denied src loads nothing).

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser, Page } from "playwright"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"

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
  await Promise.all([react?.stop(), solid?.stop()])
})

type Script<T> = (page: Page) => Promise<T>

/** One tree in both harnesses (`?view=tree`), the same script on each:
 *  returns both observations (React's first). */
async function both<T>(tree: unknown, script: Script<T>, opts: { locale?: string; dir?: `ltr` | `rtl`; clipboard?: boolean; reducedMotion?: boolean } = {}): Promise<[T, T]> {
  const run = async (h: RunningHarness) => {
    const page = await openPage(browser, h.baseUrl, { view: `tree`, tree: JSON.stringify(tree), theme: `neutral`, mode: `light`, locale: opts.locale, dir: opts.dir }, { width: 900, height: 900 }, { context: { ...(opts.clipboard ? { permissions: [`clipboard-read`, `clipboard-write`] } : {}), ...(opts.reducedMotion ? { reducedMotion: `reduce` as const } : {}) } })
    try {
      return await script(page)
    } finally {
      await page.context().close()
    }
  }
  return [await run(react), await run(solid)]
}

const log = (page: Page) => page.evaluate(() => (window as unknown as { __xuiLog: unknown[] }).__xuiLog)
const active = (page: Page) => page.evaluate(() => {
  const el = document.activeElement as HTMLElement | null
  return el ? { tag: el.tagName.toLowerCase(), part: el.dataset.xuiPart ?? null, date: el.dataset.date ?? null, label: el.getAttribute(`aria-label`) } : null
})
const stop = (page: Page) => page.evaluate(() => document.querySelector(`.xui-calendar-grid button[tabindex="0"]`)?.getAttribute(`data-date`) ?? null)
const settle = (page: Page) => page.waitForTimeout(60)
/** Past a popover's exit animation (Presence) and the focus return. */
const closed = (page: Page) => page.waitForTimeout(500)

describe(`dates: keyboard + dismissal parity`, () => {
  it(`DatePicker grid keys: arrows, PageDown, End by the locale week (de-DE), Enter picks`, async () => {
    const tree = { id: `dp`, component: `DatePicker`, props: { label: `When`, name: `when`, value: `2026-10-08` }, on: { change: { event: { name: `c` } } } }
    const [r, s] = await both(
      tree,
      async (page) => {
        const out: unknown[] = []
        await page.click(`.xui-DatePicker-trigger`)
        await settle(page)
        out.push(await active(page), await page.locator(`.xui-calendar-weekday`).first().getAttribute(`aria-label`))
        for (const key of [`ArrowRight`, `ArrowDown`, `End`, `Home`, `PageDown`, `Shift+PageUp`, `ArrowLeft`, `ArrowUp`]) {
          await page.keyboard.press(key)
          await settle(page)
          out.push([key, await active(page), await stop(page)])
        }
        await page.keyboard.press(`Enter`)
        await closed(page)
        out.push(await log(page), await page.locator(`.xui-calendar-grid`).count(), await active(page), await page.locator(`.xui-DatePicker-trigger`).textContent())
        return out
      },
      { locale: `de-DE` }
    )
    expect(r[1]).toMatch(/^Mo/)
    expect((r[2] as unknown[])[2]).toBe(`2026-10-09`)
    expect(JSON.stringify(r.at(-4))).toContain(`2025-11-04`)
    expect(s).toEqual(r)
  })
  it(`the calendar flips arrows by the SURFACE direction; Escape closes and focus returns to the trigger`, async () => {
    const tree = { id: `d`, component: `DatePicker`, props: { label: `When`, value: `2026-10-14` } }
    const [r, s] = await both(
      tree,
      async (page) => {
        await page.click(`.xui-DatePicker-trigger`)
        await settle(page)
        await page.keyboard.press(`ArrowRight`)
        await settle(page)
        const after = await active(page)
        await page.keyboard.press(`Escape`)
        await page.waitForTimeout(16)
        // Mid exit (Presence): still mounted, `data-state="closed"`.
        const exiting = await page.locator(`[data-xui-overlay="DatePicker"]`).evaluateAll((els) => els.map((e) => [e.getAttribute(`data-state`), e.getAttribute(`data-xs`)]))
        await closed(page)
        return [after, exiting, await page.locator(`.xui-calendar-grid`).count(), await active(page), await page.locator(`.xui-DatePicker-trigger`).getAttribute(`aria-expanded`)]
      },
      { dir: `rtl` }
    )
    expect((r[0] as { date: string }).date).toBe(`2026-10-13`)
    expect(r[1]).toEqual([[`closed`, null]])
    expect(r[3]).toMatchObject({ part: `DatePicker/trigger` })
    expect(s).toEqual(r)
  })
  it(`the month buttons carry the stop; Tab loops inside the popover; an outside click closes without the focus return`, async () => {
    const tree = { id: `root`, component: `Stack`, children: [{ id: `d`, component: `DatePicker`, props: { label: `When`, value: `2026-01-31`, min: `2026-02-03` } }, { id: `t`, component: `Text`, props: { text: `outside` } }] }
    const [r, s] = await both(tree, async (page) => {
      const out: unknown[] = []
      await page.click(`.xui-DatePicker-trigger`)
      await settle(page)
      out.push(await stop(page))
      await page.click(`[aria-label="Next month"]`)
      await settle(page)
      out.push(await stop(page), await page.locator(`[data-date="2026-02-02"]`).isDisabled())
      for (let i = 0; i < 4; i++) {
        await page.keyboard.press(`Tab`)
        out.push(await active(page))
      }
      await page.mouse.click(600, 600)
      await closed(page)
      out.push(await page.locator(`.xui-calendar-grid`).count(), await active(page))
      return out
    })
    expect(r[1]).toBe(`2026-02-28`)
    expect(r[2]).toBe(true)
    expect(s).toEqual(r)
  })
  it(`DateRangePicker: two steps, an earlier end swaps; the hovered range paints`, async () => {
    const tree = { id: `rp`, component: `DateRangePicker`, props: { label: `Sprint`, name: `sprint`, start: `2026-10-07`, end: `2026-10-21` }, on: { change: { event: { name: `range` } } } }
    const [r, s] = await both(tree, async (page) => {
      await page.click(`.xui-DateRangePicker-trigger`)
      await settle(page)
      await page.click(`[data-date="2026-10-20"]`)
      await settle(page)
      await page.hover(`[data-date="2026-10-12"]`)
      await settle(page)
      const pending = await page.locator(`[data-xui-overlay="DateRangePicker"]`).getAttribute(`data-pending`)
      const inRange = await page.locator(`[data-range]`).evaluateAll((els) => els.map((e) => e.getAttribute(`data-date`)))
      await page.click(`[data-date="2026-10-12"]`)
      await closed(page)
      return [pending, inRange, await log(page), await page.locator(`.xui-DateRangePicker-trigger`).textContent(), await active(page)]
    })
    expect(r[0]).toBe(`2026-10-20`)
    expect(r[1]).toEqual([`2026-10-13`, `2026-10-14`, `2026-10-15`, `2026-10-16`, `2026-10-17`, `2026-10-18`, `2026-10-19`])
    expect(JSON.stringify(r[2])).toContain(`"start":"2026-10-12","end":"2026-10-20"`)
    expect(s).toEqual(r)
  })
  it(`TimePicker: the listbox keys, type-ahead, Enter picks and the trigger takes focus back`, async () => {
    const tree = { id: `tp`, component: `TimePicker`, props: { label: `Starts`, name: `startsAt`, value: `09:30`, step: 30 }, on: { change: { event: { name: `t` } } } }
    const [r, s] = await both(tree, async (page) => {
      const out: unknown[] = []
      await page.click(`.xui-TimePicker-trigger`)
      await settle(page)
      const activeOption = () => page.evaluate(() => document.querySelector(`[role="listbox"]`)?.getAttribute(`aria-activedescendant`)?.replace(/^.*-list-/, ``) ?? null)
      out.push(await active(page), await activeOption(), await page.locator(`.xui-TimePicker-trigger`).getAttribute(`aria-expanded`))
      for (const key of [`ArrowDown`, `ArrowDown`, `ArrowUp`, `End`, `Home`, `4`]) {
        await page.keyboard.press(key)
        await settle(page)
        out.push([key, await activeOption()])
      }
      await page.keyboard.press(`Enter`)
      await closed(page)
      out.push(await log(page), await active(page), await page.locator(`.xui-TimePicker-trigger`).textContent(), await page.locator(`[role="listbox"]`).count())
      return out
    })
    expect(r[1]).toBe(`19`)
    expect(JSON.stringify(r.at(-4))).toContain(`04:00`)
    expect(s).toEqual(r)
  })
})

describe(`CodeBlock copy parity`, () => {
  it(`Copy writes the code (an ansi block without its escapes), flips to Copied and announces it`, async () => {
    const tree = {
      id: `root`,
      component: `Stack`,
      children: [
        { id: `cb`, component: `CodeBlock`, props: { code: `const x = 1 // hi\nreturn x`, language: `ts`, lineNumbers: true, highlight: [2], title: `a.ts` } },
        { id: `an`, component: `CodeBlock`, props: { code: `\u001b[31mred\u001b[0m done`, language: `ansi` } },
      ],
    }
    const [r, s] = await both(
      tree,
      async (page) => {
        const out: unknown[] = []
        for (const id of [`cb`, `an`]) {
          await page.click(`[data-xui-id="${id}.copy"]`)
          await page.waitForTimeout(120)
          out.push(await page.evaluate(() => navigator.clipboard.readText()), await page.getAttribute(`[data-xui-id="${id}.copy"]`, `aria-label`), await page.locator(`[data-xui-id="${id}.copy"] svg`).getAttribute(`data-slot`))
        }
        out.push(await page.evaluate(() => document.querySelector(`[data-xui-live="polite"]`)?.textContent ?? null))
        await page.waitForTimeout(1600)
        out.push(await page.getAttribute(`[data-xui-id="cb.copy"]`, `aria-label`))
        return out
      },
      { clipboard: true }
    )
    expect(r.slice(0, 3)).toEqual([`const x = 1 // hi\nreturn x`, `Copied`, `CodeBlock.copied`])
    expect(r[3]).toBe(`red done`)
    expect(r.at(-1)).toBe(`Copy`)
    expect(s).toEqual(r)
  })
})

describe(`media parity`, () => {
  it(`Carousel: the buttons, the arrow keys (mirrored in rtl) and the dots move the page; only the active page is placed`, async () => {
    const tree = {
      id: `cr`,
      component: `Carousel`,
      props: { page: 0, loop: true },
      on: { change: { event: { name: `page` } } },
      children: [
        { id: `s1`, component: `Text`, props: { text: `1` } },
        { id: `s2`, component: `Text`, props: { text: `2` } },
        { id: `s3`, component: `Text`, props: { text: `3` } },
      ],
    }
    for (const dir of [`ltr`, `rtl`] as const) {
      const [r, s] = await both(
        tree,
        async (page) => {
          await page.click(`[data-xui-id="cr.next"]`)
          await settle(page)
          await page.focus(`[data-xui-id="cr.next"]`)
          await page.keyboard.press(`ArrowRight`)
          await settle(page)
          await page.click(`.xui-carousel-dots button >> nth=0`)
          await settle(page)
          await page.click(`[data-xui-id="cr.previous"]`)
          await settle(page)
          const placed = await page.locator(`.xui-Carousel-page`).evaluateAll((els) => els.map((e) => [e.getAttribute(`data-xui-id`), e.hasAttribute(`inert`), e.getAttribute(`aria-hidden`)]))
          return [await log(page), placed]
        },
        { dir, reducedMotion: true }
      )
      expect((r[0] as { context: { page: number } }[]).map((e) => e.context.page)).toEqual(dir === `ltr` ? [1, 2, 0, 2] : [1, 0, 0, 2])
      expect(s).toEqual(r)
    }
  })
  it(`a denied Image / Video / AudioPlayer / Avatar src loads nothing; the Image paints its fallback + alt`, async () => {
    const tree = {
      id: `root`,
      component: `Stack`,
      children: [
        { id: `i`, component: `Image`, props: { src: `javascript:alert(1)`, alt: `Denied` } },
        { id: `f`, component: `Image`, props: { src: `file:///etc/passwd`, alt: `File` } },
        { id: `v`, component: `Video`, props: { src: `javascript:alert(1)`, poster: `file:///etc/passwd` } },
        { id: `a`, component: `AudioPlayer`, props: { src: `file:///etc/passwd` } },
        { id: `av`, component: `Avatar`, props: { name: `Jane Doe`, src: `javascript:alert(1)` } },
      ],
    }
    const [r, s] = await both(tree, async (page) => {
      await page.waitForTimeout(400)
      return page.evaluate(() => ({
        images: document.querySelectorAll(`.xui-surface img`).length,
        states: Array.from(document.querySelectorAll(`[data-xui-c="Image"]`)).map((e) => [e.getAttribute(`data-state`), e.getAttribute(`role`), e.getAttribute(`aria-label`), Boolean(e.querySelector(`.xui-Image-fallback`))]),
        video: [document.querySelector(`video`)!.hasAttribute(`src`), document.querySelector(`video`)!.hasAttribute(`poster`)],
        audio: document.querySelector(`audio`)!.hasAttribute(`src`),
        avatar: document.querySelector(`.xui-Avatar-fallback`)?.textContent ?? null,
        leaked: /javascript:|file:\/\//.test(document.querySelector(`.xui-surface`)!.innerHTML),
      }))
    })
    expect(r).toEqual({ images: 0, states: [[`error`, `img`, `Denied`, true], [`error`, `img`, `File`, true]], video: [false, false], audio: false, avatar: `JD`, leaked: false })
    expect(s).toEqual(r)
  })
})
