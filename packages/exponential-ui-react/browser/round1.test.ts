// Round 1 in headless Chromium (packages/exponential-ui/docs/round-1-contract.md
// §2, §6, §9 React): the CSS half the DOM suites cannot see —
// fixtures/style-conditions.json replayed against computed styles (container
// queries, strict max-width, height/orientation via data-xq, hover and
// reduced-motion media, the state blocks), breakpoints inside a Dialog
// (audit bug C), the kitchen sink at three widths under all three themes, and
// the keyboard model of a11y.json on the Radix-backed natives.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { builtinTheme, loadTheme, resolveStyleValues, styleToCss, BUILTIN_THEMES, BUILTIN_THEME_IDS } from "@exponential-at/ui"
import conditions from "@exponential-at/ui/fixtures/style-conditions.json"
import type { Page } from "playwright"
import { openPage, startHarness, type Harness } from "./support"

type Ctx = { label: string; width: number; height?: number; hover?: boolean; reducedMotion?: boolean; states?: string[]; breakpoints?: Record<string, number> }
type Case = { name: string; style: Record<string, unknown>; contexts: Ctx[]; expected: Record<string, unknown>[] }

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

/** Compare the node's computed style with a probe wearing the expected
 *  declarations inline, property by property. */
async function compare(page: Page, css: Record<string, string>, nodeSel: string): Promise<Record<string, [string, string]>> {
  return page.evaluate(
    ({ css, nodeSel }) => {
      const node = document.querySelector(nodeSel) as HTMLElement
      const probe = document.createElement(`div`)
      probe.className = `xui-el`
      node.parentElement!.appendChild(probe)
      for (const [k, v] of Object.entries(css)) probe.style.setProperty(k, v)
      const a = getComputedStyle(node)
      const b = getComputedStyle(probe)
      const out: Record<string, [string, string]> = {}
      for (const k of Object.keys(css)) {
        const prop = k === `transition` ? `transition-duration` : k
        out[prop] = [a.getPropertyValue(prop), b.getPropertyValue(prop)]
      }
      probe.remove()
      return out
    },
    { css, nodeSel }
  )
}

describe(`style-conditions.json in Chromium`, () => {
  const cases = conditions.cases as unknown as Case[]
  cases.forEach((c, ci) => {
    c.contexts.forEach((ctx, xi) => {
      const heightCase = Object.keys(c.style).some((k) => /height|orientation/.test(k))
      const skip = heightCase && ctx.height === undefined
      it.skipIf(skip)(`${c.name} — ${ctx.label}`, async () => {
        const context = await h.browser.newContext({ viewport: { width: Math.max(ctx.width + 40, 400), height: Math.max((ctx.height ?? 120) + 40, 400) }, reducedMotion: ctx.reducedMotion ? `reduce` : `no-preference` })
        const page = await context.newPage()
        await page.goto(h.url({ view: `conditions`, case: ci, ctx: xi }))
        await page.waitForSelector(`[data-xui-id="n"]`, { state: `attached` })
        await page.waitForTimeout(100)
        const hoverCapable = await page.evaluate(() => matchMedia(`(hover: hover)`).matches)
        if (ctx.hover !== undefined && ctx.hover !== hoverCapable) {
          await context.close()
          return // the platform's pointer is not the context's; the other context covers it
        }
        const theme = ctx.breakpoints ? loadTheme({ id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } as never, { themes: BUILTIN_THEMES }) : builtinTheme(`neutral`)
        const want = styleToCss(resolveStyleValues(theme, c.expected[xi], `light`), theme.fonts)
        if (ctx.reducedMotion) delete want.transition // reduced motion zeroes every duration (contract §2)
        const got = await compare(page, want, `[data-xui-id="n"]`)
        await context.close()
        for (const [prop, [node, probe]] of Object.entries(got)) expect(node, `${prop}`).toBe(probe)
      })
    })
  })
})

describe(`audit bug C: breakpoints match inside overlays`, () => {
  it(`a node in an open Dialog wears its @media (min-width) block`, async () => {
    const tree = { id: `d`, component: `Dialog`, props: { title: `Hi`, open: true }, children: [{ id: `inner`, component: `Text`, props: { text: `wide` }, style: { "@media (min-width: 600px)": { fontWeight: 700 } } }] }
    const page = await openPage(h, { view: `tree`, tree: JSON.stringify(tree), width: 800, theme: `neutral`, mode: `light` })
    await page.waitForSelector(`[data-xui-id="inner"]`, { state: `attached` })
    const weight = await page.evaluate(() => getComputedStyle(document.querySelector(`[data-xui-id="inner"]`)!).fontWeight)
    const inLayer = await page.evaluate(() => Boolean(document.querySelector(`[data-xui-id="inner"]`)!.closest(`.xui-container [data-xui-layer="overlay"]`)))
    await page.context().close()
    expect(inLayer).toBe(true)
    expect(weight).toBe(`700`)
  })
})

describe(`kitchen sink × themes × widths`, () => {
  for (const theme of BUILTIN_THEME_IDS) {
    for (const width of [390, 800, 1280]) {
      it(`${theme} at ${width}px: renders, no Unknown, the responsive section follows the surface`, async () => {
        const page = await openPage(h, { view: `kitchen-sink`, theme, width, mode: `dark` }, { width: width + 40, height: 900 })
        await page.waitForSelector(`[data-xui-id="resp-grid"]`)
        const r = await page.evaluate(() => {
          const q = (id: string) => document.querySelector<HTMLElement>(`[data-xui-id="${id}"]`)
          const cs = (id: string) => (q(id) ? getComputedStyle(q(id)!) : null)
          return {
            unknown: document.querySelectorAll(`[data-xui-unknown]`).length,
            columns: cs(`resp-grid`)!.gridTemplateColumns.split(` `).length,
            sidebar: cs(`resp-shell.sidebar`)?.display ?? `absent`,
            wide: cs(`resp-only-wide`)!.display,
            narrow: cs(`resp-only-narrow`)!.display,
            main: cs(`resp-main`)!.flexDirection,
            table: Boolean(q(`members-table`) ?? document.querySelector(`[data-xui-c="Table"]`)),
            code: Boolean(document.querySelector(`[data-xui-c="CodeBlock"] [data-r-kind="keyword"]`)),
            charts: document.querySelectorAll(`[data-xui-c="Chart"] svg.xui-chart-svg`).length,
            posts: Array.from(document.querySelectorAll<HTMLElement>(`[data-xui-id^="list-item."]`)).filter((e) => /^list-item\.\d+$/.test(e.dataset.xuiId!)).length,
          }
        })
        await page.context().close()
        expect(r.unknown).toBe(0)
        expect(r.columns).toBe(width >= 1024 ? 3 : width >= 640 ? 2 : 1)
        expect(r.sidebar === `none` || r.sidebar === `absent`).toBe(width < 768)
        expect(r.wide === `none`).toBe(width < 768)
        expect(r.narrow === `none`).toBe(width >= 768)
        expect(r.main).toBe(width >= 768 ? `row` : `column`)
        expect(r.table).toBe(true)
        expect(r.code).toBe(true)
        expect(r.charts).toBeGreaterThanOrEqual(3)
        expect(r.posts).toBe(3)
      })
    }
  }
})

const tree = (node: unknown, data: unknown = {}) => ({ view: `tree`, tree: JSON.stringify(node), data: JSON.stringify(data), theme: `neutral`, mode: `light`, width: 700 })
const focused = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.textContent?.trim() ?? ``)
/** Radix moves roving focus in a timeout: press, then let it land. */
async function press(page: Page, key: string) {
  await page.keyboard.press(key)
  await page.waitForTimeout(80)
}

describe(`keyboard (a11y.json) in Chromium`, () => {
  it(`Tabs: ArrowRight activates the next tab and wraps; Home/End`, async () => {
    const page = await openPage(h, tree({ id: `t`, component: `Tabs`, props: { tabs: [{ label: `One`, value: `1` }, { label: `Two`, value: `2` }, { label: `Three`, value: `3` }] }, children: [1, 2, 3].map((i) => ({ id: `p${i}`, component: `Text`, props: { text: `panel ${i}` } })) }))
    await page.focus(`[role="tab"]`)
    await press(page, `ArrowRight`)
    expect(await focused(page)).toBe(`Two`)
    expect(await page.getAttribute(`[role="tab"][aria-selected="true"]`, `id`)).toBeTruthy()
    expect(await page.textContent(`[role="tab"][aria-selected="true"]`)).toBe(`Two`)
    await press(page, `End`)
    await press(page, `ArrowRight`)
    expect(await focused(page)).toBe(`One`)
    await page.context().close()
  })
  it(`Radio and Segmented: a roving tab stop, arrows move and wrap`, async () => {
    const page = await openPage(
      h,
      tree({
        id: `root`,
        component: `Box`,
        children: [
          { id: `r`, component: `Radio`, props: { name: `size`, label: `Size`, options: [{ label: `S`, value: `s` }, { label: `M`, value: `m` }], value: `s` } },
          { id: `g`, component: `Segmented`, props: { items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: `a` } },
        ],
      })
    )
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
    await page.focus(`[data-xui-id="g"] button`)
    await press(page, `ArrowRight`)
    expect(await focused(page)).toBe(`B`)
    await press(page, `ArrowRight`)
    expect(await focused(page)).toBe(`A`)
    await page.context().close()
  })
  it(`Accordion: ArrowDown/Home/End move between headers, Enter toggles`, async () => {
    const page = await openPage(h, tree({ id: `a`, component: `Accordion`, props: { items: [{ title: `First`, value: `1` }, { title: `Second`, value: `2` }] }, children: [{ id: `b1`, component: `Text`, props: { text: `one` } }, { id: `b2`, component: `Text`, props: { text: `two` } }] }))
    await page.focus(`.xui-Accordion-trigger`)
    await press(page, `ArrowDown`)
    expect(await focused(page)).toBe(`Second`)
    await press(page, `Home`)
    expect(await focused(page)).toBe(`First`)
    await press(page, `Enter`)
    expect(await page.getAttribute(`.xui-Accordion-trigger >> nth=0`, `aria-expanded`)).toBe(`true`)
    await page.context().close()
  })
  it(`Menu (press): Enter opens, arrows move, ArrowRight opens a submenu, a checkbox item toggles`, async () => {
    const page = await openPage(h, tree({ id: `m`, component: `Menu`, props: { label: `View`, items: [{ label: `Copy`, value: `copy` }, { kind: `checkbox`, label: `Done`, value: `done`, checked: { path: `/done` } }, { kind: `submenu`, label: `More`, items: [{ label: `Deep`, value: `deep` }] }] }, on: { select: { event: { name: `sel` } } } }, { done: false }))
    await page.focus(`.xui-Menu-trigger`)
    await press(page, `Enter`)
    await page.waitForSelector(`[role="menu"]`)
    await press(page, `ArrowDown`)
    expect(await focused(page)).toContain(`Done`)
    await press(page, `Enter`)
    expect(await page.evaluate(() => (window.__xuiData!() as { done: boolean }).done)).toBe(true)
    await press(page, `ArrowDown`)
    await press(page, `ArrowRight`)
    await page.waitForSelector(`[data-xui-overlay="Menu.sub"]`)
    expect(await focused(page)).toBe(`Deep`)
    await page.context().close()
  })
  it(`Tooltip shows on keyboard focus; Popover openOn hover opens on hover and on focus`, async () => {
    const page = await openPage(
      h,
      tree({
        id: `root`,
        component: `Box`,
        style: { display: `flex`, gap: 40, padding: 80 },
        children: [
          { id: `tip`, component: `Tooltip`, props: { content: `Hint text` }, children: [{ id: `tip-t`, component: `Text`, props: { text: `Hover me` } }] },
          { id: `hc`, component: `Popover`, props: { openOn: `hover` }, slots: { trigger: { id: `hc-t`, component: `Button`, props: { label: `Card` } } }, children: [{ id: `hc-c`, component: `Text`, props: { text: `Card body` } }] },
        ],
      })
    )
    await press(page, `Tab`)
    await page.waitForSelector(`[data-xui-overlay="Tooltip"]`, { timeout: 3000 })
    await page.hover(`[data-xui-id="hc-t"]`)
    await page.waitForSelector(`[data-xui-overlay="Popover"][data-open-on="hover"]`, { timeout: 3000 })
    await page.mouse.move(1, 1)
    await page.waitForSelector(`[data-xui-overlay="Popover"]`, { state: `detached`, timeout: 3000 })
    await page.focus(`[data-xui-id="hc-t"]`)
    await page.waitForSelector(`[data-xui-overlay="Popover"]`, { timeout: 3000 })
    await page.context().close()
  })
  it(`Select (plain): ArrowDown opens on the list, Enter chooses; Menu (contextmenu): Shift+F10 opens`, async () => {
    const page = await openPage(
      h,
      tree({
        id: `root`,
        component: `Box`,
        style: { display: `flex`, flexDirection: `column`, gap: 16 },
        children: [
          { id: `s`, component: `Select`, props: { name: `s`, label: `Pick`, options: [{ label: `Alpha`, value: `a` }, { label: `Beta`, value: `b` }] }, on: { change: { event: { name: `pick` } } } },
          { id: `cm`, component: `Menu`, props: { openOn: `contextmenu`, items: [{ label: `Rename`, value: `rename` }] }, children: [{ id: `area`, component: `Box`, props: { pressable: true }, style: { width: 120, height: 40 } }] },
        ],
      })
    )
    await page.focus(`.xui-Select-trigger`)
    await press(page, `ArrowDown`)
    await page.waitForSelector(`[role="listbox"]`)
    await press(page, `ArrowDown`)
    await press(page, `Enter`)
    await page.waitForTimeout(100)
    const log = (await page.evaluate(() => window.__xuiLog)) as { action: string; context: { value: string } }[]
    expect(log.find((l) => l.action === `pick`)?.context.value).toBe(`b`)
    await page.focus(`[data-xui-id="area"]`)
    await press(page, `Shift+F10`)
    await page.waitForSelector(`[data-xui-overlay="Menu"][data-open-on="contextmenu"]`, { timeout: 3000 })
    await page.context().close()
  })
  it(`Toast sits in the toast layer above an open Dialog, bottom-centre on phones, bottom-end when wide`, async () => {
    const node = { id: `root`, component: `Box`, children: [{ id: `d`, component: `Dialog`, props: { title: `Modal`, open: true }, children: [] }, { id: `t`, component: `Toast`, props: { title: `Saved`, duration: 0 } }] }
    for (const [vw, edge] of [
      [390, `centre`],
      [1200, `end`],
    ] as const) {
      const page = await openPage(h, { ...tree(node), width: vw }, { width: vw, height: 700 })
      await page.waitForSelector(`[data-xui-id="t"]`, { state: `attached` })
      const r = await page.evaluate(() => {
        const t = document.querySelector(`[data-xui-id="t"]`)!.getBoundingClientRect()
        const top = document.elementFromPoint(t.left + t.width / 2, t.top + t.height / 2)
        return { left: t.left, right: t.right, top: Boolean(top?.closest(`[data-xui-id="t"]`)), vw: innerWidth }
      })
      await page.context().close()
      expect(r.top).toBe(true)
      if (edge === `centre`) expect(Math.abs(r.left - (r.vw - r.right))).toBeLessThanOrEqual(2)
      else expect(r.vw - r.right).toBeLessThan(r.left)
    }
  })
})

describe(`round 3 (VAPP-102) in Chromium`, () => {
  it(`Segmented bar: full width, tabBar tall, icon above the label, arrows rove`, async () => {
    const page = await openPage(
      h,
      tree({ id: `root`, component: `Box`, style: { display: `flex`, flexDirection: `column`, width: 390 }, children: [{ id: `tb`, component: `Segmented`, props: { variant: `bar`, items: [{ label: `Inbox`, value: `inbox`, icon: `nav-inbox` }, { label: `Issues`, value: `issues`, icon: `nav-issues` }], value: `inbox` } }] })
    )
    const r = await page.evaluate(() => {
      const bar = document.querySelector(`[data-xui-id="tb"]`)!.getBoundingClientRect()
      const item = document.querySelector(`[data-xui-id="tb"] .xui-Segmented-item`)!
      const icon = item.querySelector(`.xui-Segmented-icon`)!.getBoundingClientRect()
      const label = item.querySelector(`.xui-Segmented-label`)!.getBoundingClientRect()
      return { w: bar.width, h: bar.height, iconAbove: icon.bottom <= label.top + 0.5, centred: Math.abs(icon.left + icon.width / 2 - (label.left + label.width / 2)) < 1, role: document.querySelector(`[data-xui-id="tb"]`)!.tagName }
    })
    expect(r).toEqual({ w: 390, h: 56, iconAbove: true, centred: true, role: `NAV` })
    await page.focus(`[data-xui-id="tb"] [aria-current="page"]`)
    await press(page, `ArrowRight`)
    expect(await focused(page)).toBe(`Issues`)
    await press(page, `Enter`)
    expect(await page.getAttribute(`[data-xui-id="tb"] [aria-current="page"]`, `aria-label`)).toBeNull()
    expect(await page.textContent(`[data-xui-id="tb"] [aria-current="page"]`)).toBe(`Issues`)
    await page.context().close()
  })
  it(`TreeGuides geometry: column 14, line at i·14+7, elbow stub to i·14+14, rounded corner, a 1 px bridge above the part's top`, async () => {
    const rows = [0, 1, 2, 1].map((depth, i) => ({ id: `r${i}`, component: `Row`, props: { title: `Row ${i}`, depth } }))
    const page = await openPage(h, tree({ id: `sec`, component: `Section`, props: { title: `Tree`, tree: true }, style: { width: 320 }, children: rows }))
    const g = await page.evaluate(() => {
      const box = (el: Element) => el.getBoundingClientRect()
      const guides = document.querySelector(`[data-xui-id="r2"] [data-xui-c="TreeGuides"]`)!
      const origin = box(guides)
      const row = box(document.querySelector(`[data-xui-id="r2"]`)!)
      const pass = box(guides.querySelector(`[data-col="0"] [data-vertical]`)!)
      const elbowEl = guides.querySelector(`[data-col="1"] [data-elbow]`)!
      const elbow = box(elbowEl)
      const cs = getComputedStyle(elbowEl)
      const divider = document.querySelector(`[data-xui-c="List"] [data-xui-part="List/divider"]`)
      return {
        width: origin.width,
        passX: pass.left - origin.left + pass.width / 2,
        passTop: pass.top - origin.top,
        rowPad: origin.top - row.top,
        elbowX: elbow.left - origin.left + parseFloat(cs.borderLeftWidth) / 2,
        elbowEnd: elbow.right - origin.left,
        elbowTop: elbow.top - origin.top,
        elbowMid: elbow.bottom - parseFloat(cs.borderBottomWidth) / 2 - (origin.top + origin.height / 2),
        radius: cs.borderBottomLeftRadius,
        hasDivider: divider !== null,
        overflow: getComputedStyle(guides).overflow,
      }
    })
    await page.context().close()
    expect(g.width).toBe(28)
    expect(g.passX).toBeCloseTo(7, 1)
    expect(g.elbowX).toBeCloseTo(21, 1)
    expect(g.elbowEnd).toBeCloseTo(28, 1)
    expect(g.passTop).toBeCloseTo(-1, 1)
    expect(g.elbowTop).toBeCloseTo(-1, 1)
    expect(Math.abs(g.elbowMid)).toBeLessThanOrEqual(1)
    expect(g.radius).toBe(`3px`)
    expect(g.hasDivider).toBe(true)
    expect(g.overflow).toBe(`visible`)
    // The part is the Row's CONTENT box: the bridge is measured from its top
    // (the Row's flat paddingVertical = $spacing.xs sits above it).
    expect(g.rowPad).toBe(4)
  })
})
