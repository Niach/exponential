// Round 2 (packages/exponential-ui/docs/round-2-contract.md §9 React): the
// renderer half in jsdom — lifted templates, Resizable (the fixture's key
// cases replayed through the painter), the surface Formatter (Table cells,
// format functions, pickers, the chart summary), the eight new string ids,
// the display of non-strings, per-node direction, the new style keys as
// CSS, List sections / sticky headers / one-axis windowing / scrollToIndex,
// the placed part ids (§6) and the §7 web-side sizes. The Chromium half is
// browser/round2.test.ts.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { createRef } from "react"
import { ANIMATION_NAMES, CORE_CATALOG_ID, ExponentialHost, MemoryTransport, intlFormatter, reduceNested, reduceSurface } from "@exponential-at/ui"
import type { FlatComponent, NestedNode } from "@exponential-at/ui"
import resizableFixture from "@exponential-at/ui/fixtures/resizable.json"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import { ExponentialSurface, type SurfaceHandle } from "./surface"
import { BASE_CSS } from "./base-css"
import { compileNodeSheet } from "./box-css"
import { compileTheme, declarations } from "./theme-css"
import { CLIENT_FUNCTIONS, memoFormatter } from "./data"
import { defineReactExtension } from "./extensions"
import { useHostSurface } from "./host-surface"
import type { SurfaceActionEvent } from "./host"
import { builtinTheme } from "@exponential-at/ui"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const reduced = (node: NestedNode, strict = true) => {
  const r = reduceNested(node, { catalogId: CORE_CATALOG_ID })
  if (strict) expect(r.issues).toEqual([])
  return { root: r.root, templates: r.templates }
}
const byId = (c: Element | Document, id: string) => c.querySelector<HTMLElement>(`[data-xui-id="${id}"]`)

describe(`lifted templates (§4)`, () => {
  it(`a template node renders once per item and never in place`, () => {
    const nested = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `l`, component: `List`, template: { component: `row`, path: `/rows`, key: `id` }, children: [] },
        { id: `row`, component: `Text`, props: { text: { path: `name` } } },
      ],
    } as unknown as NestedNode
    const r = reduced(nested)
    expect(Object.keys(r.templates ?? {})).toEqual([`row`])
    const { container } = render(<ExponentialSurface {...r} data={{ rows: [{ id: `a`, name: `A` }, { id: `b`, name: `B` }] }} theme="neutral" id="lt" />)
    const ids = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-c="Text"]`)).map((e) => e.dataset.xuiId)
    expect(ids).toEqual([`row.a`, `row.b`])
  })
  it(`the flat path: a template component nothing else references still renders`, () => {
    const components: FlatComponent[] = [
      { id: `root`, component: `List`, children: { componentId: `item`, path: `/xs` } },
      { id: `item`, component: `Text`, text: { path: `t` } },
    ]
    const r = reduceSurface(components, { catalogId: CORE_CATALOG_ID })
    const { container } = render(<ExponentialSurface root={r.root} templates={r.templates} data={{ xs: [{ t: `one` }, { t: `two` }] }} theme="neutral" id="lf" />)
    expect(container.textContent).toContain(`one`)
    expect(container.textContent).toContain(`two`)
  })
  it(`the kitchen sink's list renders its items`, () => {
    const r = reduceNested(kitchenSink as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
    const { container } = render(<ExponentialSurface root={r.root} templates={r.templates} data={{ posts: [{ id: `1`, title: `First` }, { id: `2`, title: `Second` }] }} theme="neutral" id="ksl" />)
    expect(container.querySelector(`[data-xui-unknown]`)).toBeNull()
  })
})

interface KeyCase {
  name: string
  sizes: number[]
  handle: number
  key: string
  orientation: `horizontal` | `vertical`
  direction: `ltr` | `rtl`
  panels?: unknown[]
  expected: number[]
}

describe(`Resizable (§1)`, () => {
  const panels = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `p${i}`, component: `Text`, props: { text: `Panel ${i}` } }))
  for (const c of (resizableFixture as unknown as { keys: KeyCase[] }).keys) {
    it(`keys: ${c.name} (fixture replay through the painter)`, () => {
      const r = reduced({ id: `rz`, component: `Resizable`, props: { direction: c.orientation, sizes: { path: `/sizes` }, ...(c.panels ? { panels: c.panels } : {}) }, children: panels(c.sizes.length) } as unknown as NestedNode)
      const events: SurfaceActionEvent[] = []
      const { container } = render(<ExponentialSurface {...r} data={{ sizes: c.sizes }} theme="neutral" id="rz" direction={c.direction} host={{ onAction: (e) => void events.push(e) }} />)
      const handle = byId(container, `rz.handle.${c.handle}`)!
      fireEvent.keyDown(handle, { key: c.key })
      const sizes = c.sizes.map((_, i) => Number(byId(container, `rz.panel.${i}`)!.dataset.size))
      expect(sizes).toEqual(c.expected)
    })
  }
  it(`handles: separator a11y, $string.resize, bound sizes written, change {sizes}`, () => {
    const r = reduced({ id: `rz`, component: `Resizable`, props: { sizes: { path: `/s` }, panels: [{ min: 20, max: 80 }, {}], handle: true }, on: { change: { event: { name: `resized` } } }, children: panels(2) } as unknown as NestedNode)
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface {...r} data={{ s: [30, 70] }} theme="neutral" id="rz2" strings={{ resize: `Größe ändern` }} host={{ onAction: (e) => void events.push(e) }} />)
    const handle = container.querySelector<HTMLElement>(`[role="separator"]`)!
    expect(handle.getAttribute(`aria-orientation`)).toBe(`vertical`)
    expect(handle.getAttribute(`aria-valuenow`)).toBe(`30`)
    expect(handle.getAttribute(`aria-valuemin`)).toBe(`20`)
    expect(handle.getAttribute(`aria-valuemax`)).toBe(`80`)
    expect(handle.getAttribute(`aria-controls`)).toBe(`rz.panel.0`)
    expect(handle.getAttribute(`aria-label`)).toBe(`Größe ändern`)
    expect(container.querySelector(`.xui-Resizable-grip`)).not.toBeNull()
    expect(container.querySelector(`[role="group"][data-xui-c="Resizable"]`)).not.toBeNull()
    fireEvent.keyDown(handle, { key: `ArrowRight` })
    expect(events.at(-1)).toMatchObject({ name: `resized`, context: { sizes: [40, 60] } })
    expect(handle.getAttribute(`aria-valuenow`)).toBe(`40`)
    // flex-grow factors = the sizes (panelExtents on a 0 basis)
    expect(byId(container, `rz.panel.0`)!.style.flex).toBe(`40 1 0px`)
  })
  it(`a pointer drag resizes from the START sizes and commits once at the end`, () => {
    const r = reduced({ id: `root`, component: `Box`, children: [{ id: `rz`, component: `Resizable`, props: { sizes: { path: `/s` } }, on: { change: { event: { name: `resized` } } }, children: panels(2) }, { id: `shown`, component: `Text`, props: { text: { path: `/s/0` } } }] } as unknown as NestedNode)
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface {...r} data={{ s: [50, 50] }} theme="neutral" id="rz3" host={{ onAction: (e) => void events.push(e) }} />)
    const root = byId(container, `rz`)!
    root.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 401, bottom: 100, width: 401, height: 100, toJSON() {} }) as DOMRect
    const handle = byId(container, `rz.handle.0`)!
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 200, clientY: 10 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 220, clientY: 10 })
    expect(Number(byId(container, `rz.panel.0`)!.dataset.size)).toBe(55)
    expect(events).toHaveLength(0)
    expect(byId(container, `shown`)!.textContent).toBe(`50`)
    // The release lands before the last move has rendered: the commit still
    // takes that move's sizes.
    act(() => {
      fireEvent.pointerMove(handle, { pointerId: 1, clientX: 240, clientY: 10 })
      fireEvent.pointerUp(handle, { pointerId: 1, clientX: 240, clientY: 10 })
    })
    expect(events.map((e) => [e.name, e.context])).toEqual([[`resized`, { sizes: [60, 40] }]])
    expect(byId(container, `shown`)!.textContent).toBe(`60`)
    expect(Number(byId(container, `rz.panel.0`)!.dataset.size)).toBe(60)
  })
})

describe(`the surface Formatter (§3)`, () => {
  const columns = [
    { key: `n`, label: `N`, type: `number`, decimals: 1 },
    { key: `c`, label: `C`, type: `currency`, currency: `EUR` },
    { key: `p`, label: `P`, type: `percent` },
    { key: `d`, label: `D`, type: `date` },
    { key: `r`, label: `R`, type: `relativeTime` },
  ]
  it(`Table number / currency / percent / date / relativeTime cells in the surface locale`, () => {
    vi.useFakeTimers({ now: Date.UTC(2026, 9, 15, 12) })
    const rows = [{ id: `a`, n: 1234.56, c: 12.5, p: 0.256, d: `2026-10-14`, r: `2026-10-14T12:00:00Z` }]
    const r = reduced({ id: `t`, component: `Table`, props: { columns, rows } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="tf" locale="en-US" timeZone="UTC" />)
    const cells = (i: number) => byId(container, `t.cell.a.${i}`)!.textContent
    expect(cells(0)).toBe(`1,234.6`)
    expect(cells(1)).toBe(`€12.50`)
    expect(cells(2)).toBe(`26%`)
    expect(cells(3)).toBe(`Oct 14, 2026`)
    expect(cells(4)).toBe(`yesterday`)
  })
  it(`the locale changes them (de-DE)`, () => {
    const rows = [{ id: `a`, n: 1234.56, c: 12.5, p: 0.256, d: `2026-10-14` }]
    const r = reduced({ id: `t`, component: `Table`, props: { columns: columns.slice(0, 4), rows } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="tg" locale="de-DE" timeZone="UTC" />)
    expect(byId(container, `t.cell.a.0`)!.textContent).toBe(`1.234,6`)
    expect(byId(container, `t.cell.a.1`)!.textContent?.replace(/\s/g, ` `)).toBe(`12,50 €`)
    expect(byId(container, `t.cell.a.3`)!.textContent).toBe(`14.10.2026`)
  })
  it(`the format functions bind through it (formatPercent, formatRelativeTime, pluralize)`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `pc`, component: `Text`, props: { text: { call: `formatPercent`, args: { value: { path: `/ratio` }, decimals: 1 } } } },
        { id: `rt`, component: `Text`, props: { text: { call: `formatRelativeTime`, args: { value: `2026-10-15T10:00:00Z`, now: Date.UTC(2026, 9, 15, 12) } } } },
        { id: `pl`, component: `Text`, props: { text: { call: `pluralize`, args: { value: 0, zero: `none`, one: `one`, other: `many` } } } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} data={{ ratio: 0.1234 }} theme="neutral" id="ff" locale="en-US" timeZone="UTC" />)
    expect(byId(container, `pc`)!.textContent).toBe(`12.3%`)
    expect(byId(container, `rt`)!.textContent).toBe(`2 hours ago`)
    expect(byId(container, `pl`)!.textContent).toBe(`none`)
    expect(CLIENT_FUNCTIONS.formatNumber({ value: 1234.5 }, { data: {}, locale: `de-DE` })).toBe(`1.234,5`)
  })
  it(`NumberField shows its value through it; the chart's summary is $string.chartSummary`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `nf`, component: `NumberField`, props: { label: `Seats`, name: `seats`, value: 1234, step: 0.5 } },
        { id: `ch`, component: `Chart`, props: { kind: `bar`, categories: [`a`, `b`], series: [{ name: `Runs`, values: [1000, 2500] }] } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="nfc" locale="en-US" />)
    expect(byId(container, `nf.input`)!.getAttribute(`value`)).toBe(`1,234.0`)
    expect(container.querySelector(`svg[role="img"]`)!.getAttribute(`aria-label`)).toBe(`Chart of Runs, values from 1,000 to 2,500`)
  })
})

describe(`built-in strings replace the hard-coded labels (§3)`, () => {
  it(`invalidValue, message, codeBlock, dialog, table, carousel, slide`, () => {
    const strings = { invalidValue: `Ungültig`, message: `Nachricht`, codeBlock: `Quelltext`, dialog: `Fenster`, table: `Tabelle`, carousel: `Karussell`, slide: `Folie` }
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `in`, component: `Input`, props: { label: `Name`, name: `n`, validateOn: `change`, value: ``, checks: [{ condition: false }] } },
        { id: `cp`, component: `Composer`, props: { placeholder: `Ask…` } },
        { id: `cb`, component: `CodeBlock`, props: { code: `x`, copyable: false } },
        { id: `dl`, component: `Dialog`, props: { open: true } },
        { id: `tb`, component: `Table`, props: { columns: [{ key: `a`, label: `A` }], rows: [] } },
        { id: `cr`, component: `Carousel`, children: [{ id: `s1`, component: `Text`, props: { text: `1` } }, { id: `s2`, component: `Text`, props: { text: `2` } }] },
      ],
    } as unknown as NestedNode, false) // a check without a message (what a host or expander may send)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="st" strings={strings} />)
    fireEvent.change(container.querySelector(`input`)!, { target: { value: `a` } })
    expect(container.textContent).toContain(`Ungültig`)
    expect(container.querySelector(`textarea`)!.getAttribute(`aria-label`)).toBe(`Nachricht`)
    expect(byId(container, `cb`)!.getAttribute(`aria-label`)).toBe(`Quelltext`)
    expect(document.querySelector(`[role="dialog"]`)!.textContent).toContain(`Fenster`)
    expect(byId(container, `tb`)!.getAttribute(`aria-label`)).toBe(`Tabelle`)
    expect(byId(container, `cr`)!.getAttribute(`aria-roledescription`)).toBe(`Karussell`)
    expect(container.querySelector(`[aria-roledescription="Folie"]`)).not.toBeNull()
  })
})

describe(`display of non-strings (§3 displayString)`, () => {
  it(`a bound number shows 412, a boolean true, an object nothing`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `n`, component: `Text`, props: { text: { path: `/n` } } },
        { id: `t`, component: `Text`, props: { text: { path: `/t` } } },
        { id: `o`, component: `Text`, props: { text: { path: `/o` } } },
        { id: `m`, component: `Markdown`, props: { text: { path: `/n` } } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} data={{ n: 412, t: true, o: { a: 1 } }} theme="neutral" id="ds" />)
    expect(byId(container, `n`)!.textContent).toBe(`412`)
    expect(byId(container, `t`)!.textContent).toBe(`true`)
    expect(byId(container, `o`)!.textContent).toBe(``)
    expect(byId(container, `m`)!.textContent).toBe(`412`)
  })
})

describe(`direction on any node (§2)`, () => {
  it(`an rtl box inside an ltr surface: its dir attribute, and its natives read it`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [{ id: `inner`, component: `Box`, style: { direction: `rtl` }, children: [{ id: `tabs`, component: `Tabs`, props: { tabs: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] }, children: [] }] }],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="dr" direction="ltr" />)
    expect(byId(container, `inner`)!.getAttribute(`dir`)).toBe(`rtl`)
    expect(byId(container, `tabs`)!.getAttribute(`dir`)).toBe(`rtl`)
    expect(byId(container, `b`)!.getAttribute(`dir`)).toBeNull()
  })
  it(`mirrored glyphs follow the element's own direction (:dir)`, () => {
    expect(BASE_CSS).toMatch(/svg\[data-icon="[^"]+"\][^{]*\):dir\(rtl\)\{scale:-1 1\}/)
  })
})

describe(`style keys as CSS (§2)`, () => {
  const theme = builtinTheme(`neutral`)
  it(`backdropBlur → backdrop-filter on a $blur token; the theme defines --xui-blur-*`, () => {
    expect(declarations({ backdropBlur: `$blur.md` })).toEqual([
      [`-webkit-backdrop-filter`, `blur(var(--xui-blur-md))`],
      [`backdrop-filter`, `blur(var(--xui-blur-md))`],
    ])
    expect(compileTheme(theme).css).toContain(`--xui-blur-md:12px`)
  })
  it(`animation → the keyframe set timed by the motion tokens; animationDuration keeps the factor`, () => {
    expect(declarations({ animation: `pulse` })).toEqual([
      [`--xui-anim-factor`, `7`],
      [`animation-name`, `xui-pulse`],
      [`animation-duration`, `calc(var(--xui-motion-slow) * 7)`],
      [`animation-timing-function`, `var(--xui-ease-standard)`],
      [`animation-iteration-count`, `infinite`],
      [`animation-fill-mode`, `both`],
      [`opacity`, `var(--xui-a-opacity, 1)`],
    ])
    expect(declarations({ animation: `spin`, animationDuration: `$motion.fast` })).toContainEqual([`animation-duration`, `calc(var(--xui-motion-fast) * 4)`])
    expect(declarations({ ":hover": {}, animationDuration: `$motion.fast` } as Record<string, unknown>)).toEqual([[`animation-duration`, `calc(var(--xui-motion-fast) * var(--xui-anim-factor, 1))`]])
    for (const name of ANIMATION_NAMES) expect(BASE_CSS).toContain(`@keyframes xui-${name}{`)
    expect(BASE_CSS).toContain(`@property --xui-band`)
    expect(BASE_CSS).toContain(`@property --xui-a-opacity{syntax:"<number>";inherits:false;initial-value:1}`)
  })
  it(`an opacity animation multiplies the node's own opacity, also across a condition block`, () => {
    expect(declarations({ opacity: 0.6, animation: `fade-in` })).toContainEqual([`opacity`, `calc(0.6 * var(--xui-a-opacity, 1))`])
    // spin leaves opacity alone
    expect(declarations({ opacity: 0.4, animation: `spin` })).toContainEqual([`opacity`, `0.4`])
    const root = { id: `o`, component: `Box`, props: {}, children: [], style: { opacity: 0.5, ":hover": { animation: `pulse` }, "@media (min-width: 600px)": { opacity: 0.8 } } }
    const css = compileNodeSheet([root], `op`, theme).css
    expect(css).toMatch(/\.xui-n-o\{opacity:0\.5\}/)
    // the block adds the animation: the base's own opacity still multiplies
    expect(css).toMatch(/:hover[^{]*\{[^}]*opacity:calc\(0\.5 \* var\(--xui-a-opacity, 1\)\)/)
    // no animation anywhere: a block's opacity stays plain
    expect(css).toMatch(/@container xui \(width >= \d+px\)\{[^{]*\{opacity:0\.8\}/)
    const animated = { id: `p`, component: `Box`, props: {}, children: [], style: { animation: `fade-in`, ":hover": { opacity: 0.3 } } }
    expect(compileNodeSheet([animated], `op2`, theme).css).toMatch(/:hover[^{]*\{opacity:calc\(0\.3 \* var\(--xui-a-opacity, 1\)\)\}/)
  })
  it(`shimmer paints its band in ::after; sticky is plain CSS`, () => {
    const root = { id: `s`, component: `Box`, props: {}, children: [], style: { animation: `shimmer`, position: `sticky`, top: 0 } }
    const css = compileNodeSheet([root], `sh`, theme).css
    expect(css).toContain(`position:sticky`)
    expect(css).toMatch(/\.xui-n-s::after\{content:"";position:absolute;inset:0;[^}]*linear-gradient\(90deg, color-mix\(in srgb, var\(--xui-color-background\) 0%, transparent\) calc\(var\(--xui-band, 1\) \* 100% \+ 0%\)/)
  })
})

describe(`List (§5)`, () => {
  const items = (n: number) => Array.from({ length: n }, (_, i) => ({ id: String(i), day: i < 2 ? `Today` : `Earlier`, title: `Item ${i}` }))
  const list = (props: Record<string, unknown>, slots?: Record<string, unknown>) =>
    reduced({
      id: `root`,
      component: `Box`,
      children: [
        { id: `l`, component: `List`, props, template: { component: `it`, path: `/xs`, key: `id` }, children: [], ...(slots ? { slots } : {}) },
        { id: `it`, component: `Text`, props: { text: { path: `title` } } },
      ],
    } as unknown as NestedNode)
  it(`sectionBy: one heading (level 3) per consecutive run, the section slot bound to {value, count, index}`, () => {
    const r = list({ sectionBy: `day` }, { section: { id: `hd`, component: `Text`, props: { text: { call: `concat`, args: { values: [{ path: `value` }, ` (`, { path: `count` }, `)`] } } } } })
    const { container } = render(<ExponentialSurface {...r} data={{ xs: items(5) }} theme="neutral" id="ls" />)
    const heads = Array.from(container.querySelectorAll<HTMLElement>(`[role="heading"]`))
    expect(heads.map((h) => h.getAttribute(`aria-level`))).toEqual([`3`, `3`])
    expect(heads.map((h) => h.dataset.xuiId)).toEqual([`l.section.0`, `l.section.1`])
    expect(heads.map((h) => h.textContent)).toEqual([`Today (2)`, `Earlier (3)`])
    expect(byId(container, `hd.1`)).not.toBeNull()
  })
  it(`every item is a listitem with its position in the WHOLE list (windowed too)`, () => {
    const r = list({})
    const { container } = render(<ExponentialSurface {...r} data={{ xs: items(200) }} theme="neutral" id="lw" />)
    const li = Array.from(container.querySelectorAll<HTMLElement>(`[role="listitem"]`))
    expect(li.length).toBeGreaterThan(0)
    expect(li.length).toBeLessThan(200)
    expect(li[0].getAttribute(`aria-setsize`)).toBe(`200`)
    expect(li[0].getAttribute(`aria-posinset`)).toBe(`1`)
    expect(container.querySelector(`.xui-list-window`)!.getAttribute(`data-axis`)).toBe(`vertical`)
  })
  it(`a horizontal list windows on its own axis`, () => {
    const r = list({ direction: `horizontal` })
    const { container } = render(<ExponentialSurface {...r} data={{ xs: items(500) }} theme="neutral" id="lh" />)
    const w = container.querySelector<HTMLElement>(`.xui-list-window`)!
    expect(w.dataset.axis).toBe(`horizontal`)
    expect(Number(w.dataset.windowEnd)).toBeLessThan(500)
    expect(w.style.width).toBe(`${500 * 40}px`)
  })
  it(`stickyHeaders windows the list and keeps a header row per section`, () => {
    const r = list({ sectionBy: `day`, stickyHeaders: true })
    const { container } = render(<ExponentialSurface {...r} data={{ xs: items(6) }} theme="neutral" id="lk" />)
    expect(container.querySelector(`.xui-list-window`)).not.toBeNull()
    expect(container.querySelectorAll(`.xui-List-section`).length).toBe(2)
  })
  it(`divided: the divider before item i is <id>.divider.<i>`, () => {
    const r = list({ divided: true })
    const { container } = render(<ExponentialSurface {...r} data={{ xs: items(3) }} theme="neutral" id="ld" />)
    expect(Array.from(container.querySelectorAll<HTMLElement>(`.xui-List-divider`)).map((d) => d.dataset.xuiId)).toEqual([`l.divider.1`, `l.divider.2`])
  })
  it(`scrollToIndex is a host command on List and Table (data index)`, () => {
    const r = list({})
    const ref = createRef<SurfaceHandle>()
    const spy = vi.fn()
    const prev = window.scrollBy
    window.scrollBy = spy as unknown as typeof window.scrollBy
    try {
      render(<ExponentialSurface {...r} data={{ xs: items(300) }} theme="neutral" id="lc" handleRef={ref} />)
      expect(ref.current!.scrollToIndex(`missing`, 3)).toBe(false)
      act(() => ref.current!.run({ scrollToIndex: { id: `l`, index: 250, align: `start` } }))
      expect(spy).toHaveBeenCalledWith({ top: 250 * 40 })
    } finally {
      window.scrollBy = prev
    }
  })
})

describe(`placed parts carry data-xui-id (§6)`, () => {
  it(`Switch is label first, the track at the end; Radio rows, dots and labels per option`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `sw`, component: `Switch`, props: { label: `Notify`, name: `notify`, description: `By mail` } },
        { id: `rd`, component: `Radio`, props: { label: `Plan`, name: `plan`, options: [{ label: `Free`, value: `f` }, { label: `Team`, value: `t` }] } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="pp" />)
    const sw = byId(container, `sw`)!
    expect(Array.from(sw.children).map((e) => (e as HTMLElement).dataset.xuiId)).toEqual([`sw.body`, `sw.track`])
    expect(byId(sw, `sw.label`)!.textContent).toBe(`Notify`)
    for (const id of [`rd.label`, `rd.items`, `rd.item.0`, `rd.dot.0`, `rd.label.0`, `rd.item.1`, `rd.label.1`]) expect(byId(container, id), id).not.toBeNull()
  })
  it(`Table rows by key (duplicates → #<index>), cells, header checkbox; CodeBlock lines`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `t`, component: `Table`, props: { selectable: `multiple`, columns: [{ key: `n`, label: `N` }], rows: [{ id: `x`, n: 1 }, { id: `x`, n: 2 }], caption: `Two` } },
        { id: `c`, component: `CodeBlock`, props: { code: `a\nb`, lineNumbers: true, title: `t.ts` } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="pt" />)
    for (const id of [`t.header`, `t.headerCell.0`, `t.checkbox.header`, `t.body`, `t.row.x`, `t.row.#1`, `t.cell.x.0`, `t.cell.#1.0`, `t.checkbox.#1`, `t.caption`, `c.header`, `c.title`, `c.copy`, `c.body`, `c.line.1`, `c.lineNumber.1`, `c.code.1`]) expect(byId(container, id), id).not.toBeNull()
    const all = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-id]`)).map((e) => e.dataset.xuiId!)
    expect(new Set(all).size).toBe(all.length)
  })
  it(`Carousel: only the active page is placed; the others are inert + data-xui-inactive`, () => {
    const r = reduced({ id: `cr`, component: `Carousel`, children: [{ id: `s1`, component: `Text`, props: { text: `1` } }, { id: `s2`, component: `Text`, props: { text: `2` } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="pc" />)
    const pages = container.querySelectorAll<HTMLElement>(`.xui-Carousel-page`)
    expect(pages[0].dataset.xuiId).toBe(`cr.page`)
    expect(pages[1].dataset.xuiId).toBeUndefined()
    expect(pages[1].hasAttribute(`data-xui-inactive`)).toBe(true)
    expect(pages[1].hasAttribute(`inert`)).toBe(true)
  })
})

describe(`explicit sizes (§7)`, () => {
  it(`overlays are layout-transparent; the default menu trigger has no chevron`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `dl`, component: `Dialog`, props: { title: `T` }, slots: { trigger: { id: `dt`, component: `Button`, props: { label: `Open` } } } },
        { id: `hc`, component: `Popover`, props: { openOn: `hover` }, slots: { trigger: { id: `ht`, component: `Link`, props: { label: `who` } } } },
        { id: `nt`, component: `Popover` },
        { id: `mn`, component: `DropdownMenu`, props: { label: `More`, items: [{ label: `A` }] } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="ov" />)
    expect(byId(container, `dl`)!.getAttribute(`data-xui-overlay-root`)).toBe(``)
    expect(byId(container, `hc`)!.getAttribute(`data-xui-overlay-root`)).toBe(`stretch`)
    expect(byId(container, `nt`)!.getAttribute(`data-xui-overlay-root`)).toBe(`none`)
    expect(BASE_CSS).toContain(`[data-xui-overlay-root="none"].xui-el{display:contents}`)
    const trigger = byId(container, `mn.trigger`)!
    expect(trigger.textContent).toBe(`More`)
    expect(trigger.querySelector(`svg`)).toBeNull()
  })
  it(`Video and Image default to 16:9; Chart's height is the whole box; Accordion count is its own part`, () => {
    const r = reduced({
      id: `b`,
      component: `Box`,
      children: [
        { id: `v`, component: `Video`, props: { src: `a.mp4` } },
        { id: `i`, component: `Image`, props: { src: ``, alt: `` } },
        { id: `ch`, component: `Chart`, props: { kind: `bar`, height: 180, title: `T`, series: [{ name: `a`, values: [1] }, { name: `b`, values: [2] }] } },
        { id: `ac`, component: `Accordion`, props: { items: [{ title: `History`, value: `h`, count: 4 }] }, children: [{ id: `ac1`, component: `Text`, props: { text: `x` } }] },
        { id: `fu`, component: `FileUpload`, props: { label: `Files`, name: `files` } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="sz" />)
    expect(byId(container, `v`)!.style.aspectRatio).toBe(`1.7777778`)
    expect(byId(container, `i`)!.style.aspectRatio).toBe(`1.7777778`)
    expect(byId(container, `ch`)!.style.height).toBe(`180px`)
    expect(byId(container, `ac.count.0`)!.textContent).toBe(`4`)
    expect(byId(container, `ac.trigger.0`)!.textContent).not.toContain(`·`)
    expect(byId(container, `fu.browse`)!.textContent).toBe(`Browse`)
  })
})

describe(`round-2 review fixes`, () => {
  it(`a Table currency cell with a bad code or a non-number is empty, never a crash (the bind table's guard)`, () => {
    const columns = [
      { key: `c`, label: `C`, type: `currency`, currency: `euro` },
      { key: `d`, label: `D`, type: `currency`, currency: `EUR` },
      { key: `n`, label: `N`, type: `number` },
      { key: `p`, label: `P`, type: `percent` },
    ]
    const rows = [{ id: `a`, c: 12, d: `abc`, n: `x`, p: null }, { id: `b`, c: 1, d: `7.5`, n: `3`, p: 0.5 }]
    const r = reduced({ id: `root`, component: `Box`, children: [{ id: `t`, component: `Table`, props: { columns, rows } }, { id: `after`, component: `Text`, props: { text: `still here` } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="cg" locale="en-US" timeZone="UTC" />)
    expect(byId(container, `t.cell.a.0`)!.textContent).toBe(``)
    expect(byId(container, `t.cell.a.1`)!.textContent).toBe(``)
    expect(byId(container, `t.cell.a.2`)!.textContent).toBe(``)
    expect(byId(container, `t.cell.b.1`)!.textContent).toBe(`€7.50`)
    expect(byId(container, `t.cell.b.2`)!.textContent).toBe(`3`)
    expect(byId(container, `t.cell.b.3`)!.textContent).toBe(`50%`)
    expect(byId(container, `after`)!.textContent).toBe(`still here`)
  })
  it(`a painter that throws paints an empty box in its place; the rest of the surface stays`, () => {
    const Boom = (): never => {
      throw new Error(`boom`)
    }
    const errors = vi.spyOn(console, `error`).mockImplementation(() => {})
    try {
      const extensions = [defineReactExtension({ components: { Chart: Boom } })]
      const r = reduced({ id: `root`, component: `Box`, children: [{ id: `bad`, component: `Chart`, props: { kind: `bar`, series: [{ name: `a`, values: [1] }] } }, { id: `ok`, component: `Text`, props: { text: `fine` } }] } as unknown as NestedNode)
      const { container } = render(<ExponentialSurface {...r} theme="neutral" id="pb" extensions={extensions} />)
      expect(byId(container, `bad`)!.hasAttribute(`data-xui-error`)).toBe(true)
      expect(byId(container, `ok`)!.textContent).toBe(`fine`)
    } finally {
      errors.mockRestore()
    }
  })
  it(`a Table relativeTime column alone starts the minute clock`, () => {
    vi.useFakeTimers({ now: Date.UTC(2026, 9, 15, 12, 0, 30) })
    const r = reduced({ id: `t`, component: `Table`, props: { columns: [{ key: `r`, label: `R`, type: `relativeTime` }], rows: [{ id: `a`, r: `2026-10-15T12:00:00Z` }] } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="lc" locale="en-US" timeZone="UTC" />)
    expect(byId(container, `t.cell.a.0`)!.textContent).toBe(`30 seconds ago`)
    act(() => vi.advanceTimersByTime(120_000))
    expect(byId(container, `t.cell.a.0`)!.textContent).toBe(`2 minutes ago`)
  })
  it(`memoFormatter: the same strings, each Intl call once per input`, () => {
    let calls = 0
    const base = intlFormatter(`en-US`, `UTC`)
    const counted = { ...base, currency: (v: number, c: string, o?: { decimals?: number }) => (calls++, base.currency(v, c, o)) }
    const f = memoFormatter(counted)
    expect(f.currency(12.5, `EUR`)).toBe(base.currency(12.5, `EUR`))
    expect(f.currency(12.5, `EUR`)).toBe(`€12.50`)
    expect(f.currency(12.5, `EUR`, { decimals: 0 })).toBe(base.currency(12.5, `EUR`, { decimals: 0 }))
    expect(calls).toBe(2)
    expect(f.number(1234.5)).toBe(`1,234.5`)
    expect(f.date(`2026-10-14`)).toBe(`Oct 14, 2026`)
  })
  it(`HostSurface: a data write keeps the same lifted templates (no re-reduce per keystroke)`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    const seen: unknown[] = []
    function Probe() {
      const s = useHostSurface(host, `h`)
      if (s?.templates) seen.push(s.templates)
      return null
    }
    render(<Probe />)
    act(() => host.connect())
    act(() =>
      transport.feed(
        { version: `v0.9`, createSurface: { surfaceId: `h`, catalogId: CORE_CATALOG_ID } },
        { version: `v0.9`, updateComponents: { surfaceId: `h`, components: [{ id: `root`, component: `List`, children: { componentId: `row`, path: `/rows`, key: `id` } }, { id: `row`, component: `Text`, text: { path: `title` } }] } },
        { version: `v0.9`, updateDataModel: { surfaceId: `h`, path: `/rows`, value: [{ id: `a`, title: `A` }] } }
      )
    )
    const first = seen.at(-1)
    act(() => host.surface(`h`)!.setData(`/note`, `x`))
    act(() => host.surface(`h`)!.setData(`/note`, `xy`))
    expect(seen.length).toBeGreaterThan(1)
    expect(seen.at(-1)).toBe(first)
  })
  it(`a matching @media block's direction is the node's direction (dir, Radix, keys)`, () => {
    const r = reduced({ id: `root`, component: `Box`, children: [{ id: `wide`, component: `Box`, style: { "@media (min-width: 600px)": { direction: `rtl` } }, children: [{ id: `t`, component: `Text`, props: { text: `x` } }] }, { id: `narrow`, component: `Box`, style: { "@media (max-width: 600px)": { direction: `rtl` } } }] } as unknown as NestedNode)
    const sheet = compileNodeSheet([r.root!], `md`)
    expect(sheet.queries).toEqual([`@media (min-width: 600px)`, `@media (max-width: 600px)`])
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="md" width={800} />)
    expect(byId(container, `wide`)!.getAttribute(`dir`)).toBe(`rtl`)
    expect(byId(container, `narrow`)!.hasAttribute(`dir`)).toBe(false)
  })
  it(`a template missing from \`templates\` warns once in dev`, () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    try {
      const r = reduced({ id: `l`, component: `List`, template: { component: `gone`, path: `/rows` }, children: [] } as unknown as NestedNode, false)
      render(<ExponentialSurface root={r.root} theme="neutral" id="mt" data={{ rows: [1] }} />)
      render(<ExponentialSurface root={r.root} theme="neutral" id="mt" data={{ rows: [1, 2] }} />)
      expect(warn).toHaveBeenCalledTimes(1)
      expect(String(warn.mock.calls[0]![0])).toContain(`"gone"`)
    } finally {
      warn.mockRestore()
    }
  })
  it(`List rows: real listitem boxes; a section header is a listitem holding its heading; the divider sits in the row`, () => {
    const r = reduced({ id: `L`, component: `List`, props: { sectionBy: `day`, divided: true, gap: `md` }, template: { component: `row`, path: `/items`, key: `id` }, children: [{ id: `row`, component: `Text`, props: { text: { path: `id` } } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="ls" data={{ items: [{ id: `a`, day: `Mon` }, { id: `b`, day: `Mon` }, { id: `c`, day: `Tue` }] }} />)
    const rows = Array.from(byId(container, `L`)!.children) as HTMLElement[]
    expect(rows.map((r) => r.getAttribute(`role`))).toEqual([`listitem`, `listitem`, `listitem`, `listitem`, `listitem`])
    expect(rows[0]!.querySelector(`[role="heading"]`)).not.toBeNull()
    expect(rows[0]!.hasAttribute(`aria-posinset`)).toBe(false)
    expect(rows[2]!.getAttribute(`aria-posinset`)).toBe(`2`)
    expect(rows[2]!.querySelector(`.xui-list-gap [role="separator"]`)).not.toBeNull()
    expect(byId(container, `L`)!.style.gap).toBe(`calc(var(--xui-spacing-md) + 1px)`)
    expect(BASE_CSS).toContain(`.xui-list-row{display:flex;flex-direction:column;position:relative}`)
  })
  it(`scrollToIndex on a flat List or Table targets its OWN row, not a nested one`, () => {
    const inner = { id: `in`, component: `List`, children: [0, 1, 2, 3, 4, 5].map((i) => ({ id: `in${i}`, component: `Text`, props: { text: `inner ${i}` } })) }
    const r = reduced({ id: `root`, component: `Box`, children: [{ id: `out`, component: `List`, children: [{ id: `o0`, component: `Box`, children: [inner] }, ...[1, 2, 3, 4, 5].map((i) => ({ id: `o${i}`, component: `Text`, props: { text: `outer ${i}` } }))] }] } as unknown as NestedNode)
    const handle = createRef<SurfaceHandle>()
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="st" handleRef={handle} />)
    const spy = vi.spyOn(Element.prototype, `scrollIntoView`).mockImplementation(function (this: Element) {})
    try {
      act(() => void handle.current!.scrollToIndex(`out`, 5))
      expect(spy).toHaveBeenCalledTimes(1)
      expect((spy.mock.contexts[0] as HTMLElement).textContent).toBe(`outer 5`)
    } finally {
      spy.mockRestore()
    }
    void container
  })
  it(`date and select popovers carry the node's direction`, () => {
    const r = reduced({ id: `root`, component: `Box`, children: [{ id: `box`, component: `Box`, style: { direction: `rtl` }, children: [{ id: `dp`, component: `DatePicker`, props: { label: `When` } }] }] } as unknown as NestedNode, false)
    const { container } = render(<ExponentialSurface {...r} theme="neutral" id="dd" />)
    fireEvent.click(container.querySelector(`.xui-DatePicker-trigger`)!)
    expect(document.querySelector(`[data-xui-overlay="DatePicker"]`)!.getAttribute(`dir`)).toBe(`rtl`)
  })
})
