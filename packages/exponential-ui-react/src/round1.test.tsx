// Round 1 (packages/exponential-ui/docs/round-1-contract.md §9 React): the
// renderer half of the contract in jsdom — the bind pass against
// fixtures/bind-time.json (props, styles, recipe props, `visible`, `set`
// presses), the audit's bugs (template styles, overlays inside the
// container, bound macro inputs), conditions → CSS, responsive natives,
// strings, locale, settings (mode/density/contrast), host commands and the
// new natives. The CSS-in-a-real-browser half is browser/round1.test.ts.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { createRef } from "react"
import { builtinTheme, CORE_CATALOG_ID, DEFAULT_STRINGS, expandMacros, reduceNested } from "@exponential-at/ui"
import type { FlatComponent, NestedNode, UiNode } from "@exponential-at/ui"
import bindTime from "@exponential-at/ui/fixtures/bind-time.json"
import { ExponentialSurface, type SurfaceHandle } from "./surface"
import { compileNodeSheet, nodeSheet } from "./box-css"
import { bindTree, CLIENT_FUNCTIONS, getPointer } from "./data"
import { runNodeAction } from "./node-view"
import { compileTheme, declarations, STATE_SELECTORS } from "./theme-css"
import { useSurface } from "./use-surface"
import { parseLocaleNumber } from "./natives/inputs"
import { sortRows } from "./natives/table"
import { acceptsFile } from "./natives/chips"
import { monthGrid } from "./natives/dates"
import type { HostPlugin, SurfaceActionEvent } from "./host"
import type { SurfaceContextValue } from "./context"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const expand = (node: NestedNode) => expandMacros(reduceNested(node, { catalogId: CORE_CATALOG_ID, expand: false }).root)

interface BindCase {
  name: string
  input: NestedNode
  expanded: UiNode
  datasets: { data: Record<string, unknown>; bound: UiNode; presses: { id: string; outcome: { data: unknown; event?: { name: string; context?: Record<string, unknown> } } }[] }[]
}

function find(node: UiNode, id: string): UiNode | undefined {
  if (node.id === id) return node
  for (const c of [...node.children, ...Object.values(node.slots ?? {})]) {
    const hit = find(c, id)
    if (hit) return hit
  }
  return undefined
}

function* ids(node: UiNode): Generator<string> {
  yield node.id
  for (const c of [...node.children, ...Object.values(node.slots ?? {})]) yield* ids(c)
}

describe(`bind pass: fixtures/bind-time.json through the React resolver`, () => {
  for (const c of (bindTime as unknown as { cases: BindCase[] }).cases) {
    describe(c.name, () => {
      it(`expands as the fixture says`, () => expect(expand(c.input)).toEqual(c.expanded))
      c.datasets.forEach((ds, i) => {
        it(`dataset ${i}: the bound tree`, () => {
          expect(bindTree(c.expanded, { data: ds.data, functions: CLIENT_FUNCTIONS, strings: DEFAULT_STRINGS })).toEqual(ds.bound)
        })
        it(`dataset ${i}: every press (resolve, then set, then the event)`, () => {
          for (const press of ds.presses) {
            const node = find(c.expanded, press.id)!
            let data: unknown = ds.data
            const events: SurfaceActionEvent[] = []
            const ctx = {
              surfaceId: `b`,
              data,
              functions: CLIENT_FUNCTIONS,
              openUrl: () => {},
              locale: `en-US`,
              strings: DEFAULT_STRINGS,
              setData: (p: string, v: unknown) => {
                data = structuredClone(data) as Record<string, unknown>
                const tokens = p.slice(1).split(`/`)
                let cur = data as Record<string, unknown>
                for (const t of tokens.slice(0, -1)) cur = (cur[t] ??= {}) as Record<string, unknown>
                cur[tokens[tokens.length - 1]] = v
              },
              host: { onAction: (e: SurfaceActionEvent) => void events.push(e) },
            } as unknown as SurfaceContextValue
            runNodeAction(ctx, node, node.id, ``, `press`)
            expect(data, press.id).toEqual(press.outcome.data)
            if (press.outcome.event) expect(events[0]).toMatchObject({ name: press.outcome.event.name, ...(press.outcome.event.context ? { context: press.outcome.event.context } : {}) })
          }
        })
        it(`dataset ${i}: renders, and a node the bind pass drops is not in the DOM`, () => {
          const { container } = render(<ExponentialSurface root={c.expanded} data={ds.data} theme="neutral" id={`bt${i}`} />)
          const kept = new Set(ids(ds.bound))
          for (const id of ids(c.expanded)) if (!kept.has(id)) expect(container.querySelector(`[data-xui-id="${id}"]`), id).toBeNull()
        })
      })
    })
  }
})

describe(`the audit's bugs`, () => {
  it(`A: a bound Progress paints its fill from the data (a dynamic style value → a variable)`, () => {
    const root = tree({ id: `p`, component: `Progress`, props: { value: { path: `/v` }, max: 200 } })
    const { container } = render(<ExponentialSurface root={root} data={{ v: 50 }} theme="neutral" id="pa" />)
    const fill = container.querySelector(`[data-xui-id="p.track.fill"]`) as HTMLElement
    expect(fill.style.getPropertyValue(`--xd-0`)).toBe(`25%`)
    expect(container.querySelector(`style[data-xui-style="nodes"]`)!.textContent).toContain(`.xui-n-p_2e_track_2e_fill{height:100%;width:var(--xd-0)`)
  })
  it(`A: a bound Collapsible toggles through set (two-way) and routes the new value`, () => {
    const actions: SurfaceActionEvent[] = []
    const root = tree({ id: `c`, component: `Collapsible`, props: { title: `More`, open: { path: `/open` } }, on: { change: { event: { name: `toggled` } } }, children: [{ id: `body-text`, component: `Text`, props: { text: `Inside` } }] })
    const { container } = render(<ExponentialSurface root={root} data={{ open: false }} theme="neutral" id="ca" host={{ onAction: (e) => void actions.push(e) }} />)
    expect(container.querySelector(`[data-xui-id="body-text"]`)).toBeNull()
    fireEvent.click(container.querySelector(`[data-xui-id="c.trigger"]`)!)
    expect(container.querySelector(`[data-xui-id="body-text"]`)).not.toBeNull()
    expect(actions[0]).toMatchObject({ name: `toggled`, context: { open: true } })
    fireEvent.click(container.querySelector(`[data-xui-id="c.trigger"]`)!)
    expect(container.querySelector(`[data-xui-id="body-text"]`)).toBeNull()
  })
  it(`A: a bound Pagination prints the page and its arrows write the page`, () => {
    const root = tree({ id: `pg`, component: `Pagination`, props: { page: { path: `/page` }, totalPages: 5 } })
    const { container } = render(<ExponentialSurface root={root} data={{ page: 2 }} theme="neutral" id="pga" />)
    expect(container.textContent).toContain(`2`)
    expect(container.textContent).not.toContain(`[object Object]`)
    fireEvent.click(container.querySelector(`[data-xui-id="pg.next"]`)!)
    expect(container.textContent).toContain(`3`)
  })

  const flat: FlatComponent[] = [
    { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows`, key: `id` } },
    { id: `row`, component: `Stack`, direction: `horizontal`, gap: `md`, style: { backgroundColor: `$color.muted`, "@media (min-width: 600px)": { padding: 12 } }, children: [`row-title`, `row-input`] },
    { id: `row-title`, component: `Text`, text: { path: `title` } },
    { id: `row-input`, component: `Input`, label: `Note`, name: `note` },
  ]
  function Flat({ data }: { data: Record<string, unknown> }) {
    const s = useSurface({ surfaceId: `t`, data })
    return (
      <>
        <button data-testid="load" onClick={() => s.apply({ updateComponents: { surfaceId: `t`, components: flat } })} />
        <button data-testid="reorder" onClick={() => s.setData(`/rows`, [...(getPointer(s.data, `/rows`) as unknown[])].reverse())} />
        <ExponentialSurface surface={s} theme="neutral" />
      </>
    )
  }
  it(`B: template items wear their node's author and macro styles (the sheet covers template subtrees)`, () => {
    const { container } = render(<Flat data={{ rows: [{ id: `a`, title: `Alpha` }, { id: `b`, title: `Beta` }] }} />)
    fireEvent.click(screen.getByTestId(`load`))
    const items = container.querySelectorAll(`[data-xui-c="Box"][data-xui-id^="row."]`)
    expect(items.length).toBe(2)
    for (const el of items) expect(el.classList.contains(`xui-n-row`)).toBe(true)
    const sheet = container.querySelector(`style[data-xui-style="nodes"]`)!.textContent!
    expect(sheet).toMatch(/\.xui-n-row\{[^}]*background-color:var\(--xui-color-muted\)/)
    expect(sheet).toContain(`@container xui (width >= 600px){.xui-s-t .xui-n-row{padding:12px}}`)
    // The Stack macro's structural style (direction + gap) reaches the items too.
    expect(sheet).toMatch(/\.xui-n-row\{[^}]*flex-direction:row/)
  })
  it(`template items are keyed by template.key: a reorder keeps each item's element and local state`, () => {
    const { container } = render(<Flat data={{ rows: [{ id: `a`, title: `Alpha` }, { id: `b`, title: `Beta` }] }} />)
    fireEvent.click(screen.getByTestId(`load`))
    const a = container.querySelector(`[data-xui-id="row-input.a"] input`) as HTMLInputElement
    fireEvent.change(a, { target: { value: `typed in a` } })
    fireEvent.click(screen.getByTestId(`reorder`))
    const after = container.querySelector(`[data-xui-id="row-input.a"] input`) as HTMLInputElement
    expect(after).toBe(a)
    expect(after.value).toBe(`typed in a`)
    const titles = Array.from(container.querySelectorAll(`[data-xui-c="Text"]`)).map((e) => e.textContent)
    expect(titles).toEqual([`Beta`, `Alpha`])
  })
  it(`C: overlays portal INSIDE the xui container (breakpoints match in a Dialog)`, () => {
    const root = tree({ id: `d`, component: `Dialog`, props: { title: `Hi`, open: true }, children: [{ id: `inner`, component: `Text`, props: { text: `x` }, style: { "@media (min-width: 600px)": { color: `$color.primary` } } }] })
    render(<ExponentialSurface root={root} theme="neutral" id="dc" />)
    const inner = document.querySelector(`[data-xui-id="inner"]`) as HTMLElement
    expect(inner.closest(`.xui-container`)).not.toBeNull()
    expect(inner.closest(`[data-xui-layer="overlay"]`)).not.toBeNull()
  })
})

describe(`node sheet: conditions → CSS`, () => {
  const theme = builtinTheme(`neutral`)
  const sheet = (style: Record<string, unknown>) => compileNodeSheet([{ id: `n`, component: `Box`, props: {}, style, children: [] }], `s`, theme)
  it(`min/max width (strict) as container queries, $breakpoint from the theme, an unknown token dropped`, () => {
    const { css } = sheet({ gap: 4, "@media (min-width: $breakpoint.md)": { gap: 8 }, "@media (max-width: $breakpoint.md)": { display: `none` }, "@media (min-width: $breakpoint.huge)": { gap: 99 } })
    expect(css).toContain(`@container xui (width >= 768px){.xui-s-s .xui-n-n{gap:8px}}`)
    expect(css).toContain(`@container xui (width < 768px){.xui-s-s .xui-n-n{display:none}}`)
    expect(css).not.toContain(`99px`)
  })
  it(`height and orientation are attribute-gated (the surface lists the matching ones in data-xq)`, () => {
    const s = sheet({ "@media (min-height: 500px)": { gap: 12 }, "@media (orientation: portrait)": { flexDirection: `column` } })
    expect(s.queries).toEqual([`@media (min-height: 500px)`, `@media (orientation: portrait)`])
    expect(s.css).toContain(`.xui-s-s:where([data-xq~="min-height-500px"]) .xui-n-n{gap:12px}`)
    expect(s.css).toContain(`.xui-s-s:where([data-xq~="orientation-portrait"]) .xui-n-n{flex-direction:column}`)
  })
  it(`hover capability and reduced motion are real media queries; states come last, in order`, () => {
    const { css } = sheet({ opacity: 1, ":pressed": { opacity: 0.6 }, ":hover": { opacity: 0.8 }, ":focus-visible": { borderColor: `$color.ring` }, "@media (hover: none)": { minHeight: 44 }, "@media (prefers-reduced-motion: reduce)": { transform: `scale(1)` } })
    expect(css).toContain(`@media (hover: none){.xui-s-s .xui-n-n{min-height:44px}}`)
    expect(css).toContain(`@media (prefers-reduced-motion: reduce){.xui-s-s .xui-n-n{transform:scale(1)}}`)
    const hover = css.indexOf(`:hover,`)
    const focus = css.indexOf(`:focus-visible,`)
    const pressed = css.indexOf(`:active,`)
    expect(hover).toBeGreaterThan(css.indexOf(`@media (prefers-reduced-motion`))
    expect(hover).toBeLessThan(focus)
    expect(focus).toBeLessThan(pressed)
  })
  it(`the surface sets data-xq from its box (orientation landscape without a height)`, () => {
    const root = tree({ id: `n`, component: `Box`, style: { "@media (orientation: landscape)": { gap: 4 } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" width={500} id="xq" />)
    expect((container.firstElementChild as HTMLElement).dataset.xq).toBe(`orientation-landscape`)
  })
  it(`the new style keys: transition + easing, gradient, per-side borders, text styling`, () => {
    expect(declarations({ transition: `$motion.fast`, transitionEasing: `$ease.standard` })).toEqual([
      [`transition-property`, `all`],
      [`transition-duration`, `var(--xui-motion-fast)`],
      [`transition-timing-function`, `var(--xui-ease-standard)`],
    ])
    expect(declarations({ backgroundGradient: { angle: 90, stops: [{ color: `$color.primary`, offset: 0 }, { color: `#ffffff00`, offset: 1 }] } })).toEqual([[`background-image`, `linear-gradient(90deg, var(--xui-color-primary) 0%, #ffffff00 100%)`]])
    // Round 2: per-side widths alone keep the cascaded other sides (the base
    // layer resets a part's UA `medium`, so a node style never zeroes a recipe border).
    expect(declarations({ borderBottomWidth: 1, borderStyle: `dashed` })).toEqual([
      [`border-bottom-width`, `1px`],
      [`border-style`, `dashed`],
    ])
    expect(declarations({ borderTopWidth: 2 })).toEqual([
      [`border-top-width`, `2px`],
      [`border-style`, `solid`],
    ])
    expect(declarations({ overflowX: `hidden`, letterSpacing: 1, textTransform: `uppercase`, fontStyle: `italic`, cursor: `pointer`, pointerEvents: `none`, visibility: `hidden`, insetBlockStart: 4, borderTopLeftRadius: 6 })).toEqual([
      [`overflow-x`, `clip`],
      [`letter-spacing`, `1px`],
      [`text-transform`, `uppercase`],
      [`font-style`, `italic`],
      [`cursor`, `pointer`],
      [`pointer-events`, `none`],
      [`visibility`, `hidden`],
      [`inset-block-start`, `4px`],
      [`border-top-left-radius`, `6px`],
    ])
  })
  it(`themes: ease + breakpoint variables, invalid/dragover states, chart 6–8`, () => {
    const css = compileTheme(theme).css
    expect(css).toContain(`--xui-ease-standard:cubic-bezier(0.2, 0, 0, 1)`)
    expect(css).toContain(`--xui-breakpoint-md:768px`)
    expect(css).toContain(`--xui-color-chart8:`)
    expect(STATE_SELECTORS.invalid).toContain(`[aria-invalid="true"]`)
    expect(css).toContain(`.xui-FileUpload-dropzone[data-dragover]`)
    expect(css).toContain(`transition-duration:var(--xui-motion-fast)`)
  })
  it(`nodeSheet still accepts fonts (the VAPP-87 signature)`, () => {
    expect(nodeSheet({ id: `x`, component: `Box`, props: {}, style: { gap: 2 }, children: [] }, `q`)).toBe(`@layer xui-node{.xui-s-q .xui-n-x{gap:2px}}`)
  })
})

describe(`responsive natives, visible, strings, locale`, () => {
  it(`Drawer.side takes its value at the surface's breakpoint before the recipe sees it`, () => {
    const node: NestedNode = { id: `dr`, component: `Drawer`, props: { open: true, title: `T`, side: { base: `bottom`, md: `right` } }, children: [] }
    render(<ExponentialSurface root={tree(node)} theme="neutral" width={500} id="r1" />)
    expect(document.querySelector(`[data-xui-overlay="Drawer"]`)!.getAttribute(`data-side`)).toBe(`bottom`)
    cleanup()
    render(<ExponentialSurface root={tree(node)} theme="neutral" width={900} id="r2" />)
    expect(document.querySelector(`[data-xui-overlay="Drawer"]`)!.getAttribute(`data-side`)).toBe(`right`)
    expect(document.querySelector(`[data-xui-overlay="Drawer"]`)!.getAttribute(`data-r-side`)).toBe(`right`)
  })
  it(`a bound visible hides a node (no element), a literal false too`, () => {
    const root = tree({ id: `root`, component: `Box`, children: [{ id: `a`, component: `Text`, props: { text: `A` }, visible: { path: `/show` } }, { id: `b`, component: `Text`, props: { text: `B` }, visible: false }] })
    const { container, rerender } = render(<ExponentialSurface root={root} data={{ show: 0 }} theme="neutral" id="v" />)
    // 0 is TRUE (the catalog's truthiness).
    expect(container.querySelector(`[data-xui-id="a"]`)).not.toBeNull()
    expect(container.querySelector(`[data-xui-id="b"]`)).toBeNull()
    rerender(<ExponentialSurface root={root} data={{ show: `` }} theme="neutral" id="v2" />)
    expect(container.querySelector(`[data-xui-id="a"]`)).toBeNull()
  })
  it(`built-in strings: $string refs resolve, the host overrides any id`, () => {
    const root = tree({ id: `root`, component: `Box`, children: [{ id: `t`, component: `Text`, props: { text: `$string.cancel` } }, { id: `tb`, component: `Table`, props: { columns: [{ key: `a`, label: `A` }], rows: [] } }] })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" strings={{ cancel: `Abbrechen`, noResults: `Keine Treffer` }} id="st" />)
    expect(container.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Abbrechen`)
    expect(container.querySelector(`.xui-Table-empty`)!.textContent).toBe(`Keine Treffer`)
  })
  it(`placeholder strings: removeItem / sortByAscending name their item, English by default and overridable`, () => {
    const root = tree({ id: `root`, component: `Box`, children: [{ id: `ch`, component: `ChipInput`, props: { label: `Labels`, name: `labels`, values: [`bug`] } }, { id: `tb`, component: `Table`, props: { columns: [{ key: `a`, label: `Name`, sortable: true }], rows: [{ a: `x` }] } }] })
    const remove = (c: HTMLElement) => c.querySelector(`.xui-ChipInput-remove`)!.getAttribute(`aria-label`)
    const sort = (c: HTMLElement) => c.querySelector(`.xui-table-sort`)!.getAttribute(`aria-label`)
    const en = render(<ExponentialSurface root={root} theme="neutral" id="ph1" />)
    expect(remove(en.container)).toBe(`Remove bug`)
    expect(sort(en.container)).toBe(`Name: Sort ascending`)
    cleanup()
    const de = render(<ExponentialSurface root={root} theme="neutral" strings={{ removeItem: `{name} entfernen`, sortByAscending: `{name} aufsteigend sortieren` }} id="ph2" />)
    expect(remove(de.container)).toBe(`bug entfernen`)
    expect(sort(de.container)).toBe(`Name aufsteigend sortieren`)
  })
  it(`locale: direction from the language, numbers in the surface locale, the locale's week start`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `n`, component: `NumberField`, props: { label: `Amount`, name: `amount`, value: 1234.5, precision: 1 } })} theme="neutral" locale="de-DE" id="l1" />)
    expect((container.querySelector(`.xui-NumberField-input`) as HTMLInputElement).value).toBe(`1.234,5`)
    cleanup()
    const rtl = render(<ExponentialSurface root={tree({ id: `t`, component: `Text`, props: { text: `مرحبا` } })} theme="neutral" locale="ar-EG" id="l2" />)
    expect((rtl.container.firstElementChild as HTMLElement).getAttribute(`dir`)).toBe(`rtl`)
    expect(monthGrid(new Date(2026, 9, 1), 1)[0].getDay()).toBe(1)
    expect(monthGrid(new Date(2026, 9, 1), 0)[0].getDay()).toBe(0)
    expect(parseLocaleNumber(`1.234,5`, `de-DE`)).toBe(1234.5)
    expect(parseLocaleNumber(`1,234.5`, `en-US`)).toBe(1234.5)
  })
  it(`DatePicker: weekday header starts on the locale's first day; firstDayOfWeek overrides`, () => {
    const node = (extra: Record<string, unknown>): NestedNode => ({ id: `dp`, component: `DatePicker`, props: { label: `When`, name: `when`, value: `2026-10-08`, ...extra } })
    const first = (locale: string, extra: Record<string, unknown> = {}) => {
      const r = render(<ExponentialSurface root={tree(node(extra))} theme="neutral" locale={locale} id={`dp${locale}`} />)
      fireEvent.click(r.container.querySelector(`.xui-DatePicker-trigger`)!)
      const head = document.querySelector(`.xui-calendar-weekday`)!.getAttribute(`aria-label`)
      cleanup()
      return head
    }
    expect(first(`en-US`)).toBe(`Sun`)
    expect(first(`de-DE`)).toMatch(/^Mo/)
    expect(first(`en-US`, { firstDayOfWeek: 1 })).toBe(`Mon`)
  })
  it(`DatePicker grid keys: arrows, PageDown, End by the locale week, Enter picks`, () => {
    const changes: unknown[] = []
    const root = tree({ id: `dp`, component: `DatePicker`, props: { label: `When`, name: `when`, value: `2026-10-08` }, on: { change: { event: { name: `c` } } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" locale="de-DE" id="dpk" host={{ onAction: (e) => void changes.push(e.context.value) }} />)
    fireEvent.click(container.querySelector(`.xui-DatePicker-trigger`)!)
    const grid = document.querySelector(`.xui-calendar-grid`)!
    const focused = () => document.querySelector(`.xui-calendar-grid button[tabindex="0"]`)!.getAttribute(`data-date`)
    expect(focused()).toBe(`2026-10-08`)
    fireEvent.keyDown(grid, { key: `ArrowRight` })
    expect(focused()).toBe(`2026-10-09`)
    fireEvent.keyDown(grid, { key: `ArrowDown` })
    expect(focused()).toBe(`2026-10-16`)
    fireEvent.keyDown(grid, { key: `End` })
    expect(focused()).toBe(`2026-10-18`) // Sunday ends a Monday-first week
    fireEvent.keyDown(grid, { key: `PageDown` })
    expect(focused()).toBe(`2026-11-18`)
    fireEvent.keyDown(grid, { key: `Enter` })
    expect(changes).toEqual([`2026-11-18`])
  })
})

describe(`surface settings and host commands`, () => {
  it(`mode system follows prefers-color-scheme; density and contrast change the theme`, () => {
    const mm = vi.fn((q: string) => ({ matches: q.includes(`dark`) || q.includes(`more`), media: q, addEventListener: () => {}, removeEventListener: () => {} }))
    vi.stubGlobal(`matchMedia`, mm)
    window.matchMedia = mm as unknown as typeof window.matchMedia
    const root = tree({ id: `t`, component: `Text`, props: { text: `x` } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" density="compact" id="m" />)
    const surface = container.firstElementChild as HTMLElement
    expect(surface.dataset.xuiMode).toBe(`dark`)
    const css = container.querySelector(`style[data-xui-style="theme"]`)!.textContent!
    expect(css).toContain(`--xui-control-input:${Math.round(36 * 0.875)}px`)
    // neutral's high-contrast dark overlay: foreground #ffffff, border #a3a3a3.
    expect(css).toMatch(/\[data-xui-mode="dark"\]\{[^}]*--xui-color-border:#a3a3a3/)
    vi.unstubAllGlobals()
    delete (window as { matchMedia?: unknown }).matchMedia
  })
  it(`handleRef: focus a node, announce through the live region, scrollIntoView`, () => {
    vi.useFakeTimers()
    const ref = createRef<SurfaceHandle>()
    const root = tree({ id: `root`, component: `Box`, children: [{ id: `in`, component: `Input`, props: { label: `Name`, name: `name` } }] })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" handleRef={ref} id="cmd" />)
    act(() => ref.current!.run({ focus: { id: `in` } }))
    expect(document.activeElement).toBe(container.querySelector(`input`))
    act(() => ref.current!.run({ announce: { text: `Saved`, live: `assertive` } }))
    act(() => vi.advanceTimersByTime(50))
    expect(container.querySelector(`[data-xui-live="assertive"]`)!.textContent).toBe(`Saved`)
    expect(ref.current!.scrollIntoView(`in`)).toBe(true)
    expect(ref.current!.focus(`missing`)).toBe(false)
  })
  it(`Text live becomes an aria-live region`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `t`, component: `Text`, props: { text: `3 new`, live: `polite` } })} theme="neutral" id="lv" />)
    expect(container.querySelector(`[data-xui-id="t"]`)!.getAttribute(`aria-live`)).toBe(`polite`)
  })
})

describe(`Form`, () => {
  const form = (extra: Record<string, unknown> = {}): NestedNode => ({
    id: `f`,
    component: `Form`,
    props: { name: `signup`, summary: true, ...extra },
    on: { submit: { event: { name: `submit` } }, invalid: { event: { name: `invalid` } } },
    children: [
      {
        id: `email`,
        component: `Input`,
        props: {
          label: `Email`,
          name: `email`,
          value: { path: `/email` },
          checks: [
            { condition: { call: `required`, args: { value: { path: `/email` } } }, message: `Required` },
            { condition: { call: `email`, args: { value: { path: `/email` } } }, message: `Not an email` },
          ],
        },
      },
      { id: `age`, component: `NumberField`, props: { label: `Age`, name: `age`, value: { path: `/age` }, checks: [{ condition: { call: `numeric`, args: { value: { path: `/age` }, min: 18 } }, message: `18+` }] } },
      { id: `go`, component: `Button`, props: { label: `Send`, submit: true } },
    ],
  })
  it(`a failed submit: invalid {errors}, ALL messages under each field, linked, summary, focus, announce`, () => {
    vi.useFakeTimers()
    const actions: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={tree(form())} data={{ email: ``, age: 12 }} theme="neutral" id="fm" host={{ onAction: (e) => void actions.push(e) }} />)
    fireEvent.click(container.querySelector(`[data-xui-id="go"]`)!)
    const invalid = actions.find((a) => a.name === `invalid`)!
    expect(invalid.context.errors).toEqual([
      { name: `email`, message: `Required` },
      { name: `email`, message: `Not an email` },
      { name: `age`, message: `18+` },
    ])
    expect(actions.some((a) => a.name === `submit`)).toBe(false)
    const err = container.querySelector(`[id="email.error"]`)!
    expect(Array.from(err.querySelectorAll(`.xui-field-error`)).map((e) => e.textContent)).toEqual([`Required`, `Not an email`])
    expect(err.getAttribute(`role`)).toBe(`alert`)
    const input = container.querySelector(`[data-xui-id="email"] input`)!
    expect(input.getAttribute(`aria-describedby`)).toContain(`email.error`)
    expect(input.getAttribute(`aria-invalid`)).toBe(`true`)
    expect(document.activeElement).toBe(input)
    expect(container.querySelector(`.xui-Form-summary`)!.textContent).toContain(`18+`)
    act(() => vi.advanceTimersByTime(50))
    expect(container.querySelector(`[data-xui-live="assertive"]`)!.textContent).toBe(`Check the highlighted fields`)
  })
  it(`a valid submit sends {values}; Enter in a field submits; busy refuses`, () => {
    const actions: SurfaceActionEvent[] = []
    const host: HostPlugin = { onAction: (e) => void actions.push(e) }
    const { container, rerender } = render(<ExponentialSurface root={tree(form())} data={{ email: `a@b.co`, age: 30 }} theme="neutral" id="fv" host={host} />)
    fireEvent.keyDown(container.querySelector(`[data-xui-id="email"] input`)!, { key: `Enter` })
    expect(actions.find((a) => a.name === `submit`)!.context.values).toEqual({ email: `a@b.co`, age: 30 })
    actions.length = 0
    rerender(<ExponentialSurface root={tree(form({ busy: true }))} data={{ email: `a@b.co`, age: 30 }} theme="neutral" id="fv" host={host} />)
    const go = container.querySelector(`[data-xui-id="go"]`) as HTMLButtonElement
    expect(go.getAttribute(`aria-busy`)).toBe(`true`)
    fireEvent.keyDown(container.querySelector(`[data-xui-id="email"] input`)!, { key: `Enter` })
    expect(actions.filter((a) => a.name === `submit`)).toEqual([])
  })
  it(`disabled makes every field inert`, () => {
    const { container } = render(<ExponentialSurface root={tree(form({ disabled: true }))} data={{ email: ``, age: 1 }} theme="neutral" id="fd" />)
    expect((container.querySelector(`fieldset`) as HTMLFieldSetElement).disabled).toBe(true)
    expect((container.querySelector(`[data-xui-id="email"] input`) as HTMLInputElement).disabled).toBe(true)
  })
})

describe(`new natives`, () => {
  it(`NumberField: spinbutton, ArrowUp/Shift, clamps to max, writes the bound path`, () => {
    const actions: SurfaceActionEvent[] = []
    const root = tree({ id: `n`, component: `NumberField`, props: { label: `Seats`, name: `seats`, value: { path: `/seats` }, min: 1, max: 30, step: 1 }, on: { change: { event: { name: `c` } } } })
    const { container } = render(<ExponentialSurface root={root} data={{ seats: 5 }} theme="neutral" id="nf" host={{ onAction: (e) => void actions.push(e) }} />)
    const input = container.querySelector(`input`)!
    expect(input.getAttribute(`role`)).toBe(`spinbutton`)
    fireEvent.keyDown(input, { key: `ArrowUp` })
    expect(actions.at(-1)!.context.value).toBe(6)
    fireEvent.keyDown(input, { key: `ArrowUp`, shiftKey: true })
    expect(actions.at(-1)!.context.value).toBe(16)
    fireEvent.keyDown(input, { key: `End` })
    expect(actions.at(-1)!.context.value).toBe(30)
    fireEvent.click(container.querySelector(`.xui-NumberField-decrement`)!)
    expect(actions.at(-1)!.context.value).toBe(29)
    expect(input.getAttribute(`aria-valuemax`)).toBe(`30`)
  })
  it(`ChipInput: Enter and comma add, Backspace focuses then removes the last chip`, () => {
    const events: [string, unknown][] = []
    const root = tree({ id: `ch`, component: `ChipInput`, props: { label: `Labels`, name: `labels`, values: [`bug`] }, on: { add: { event: { name: `add` } }, remove: { event: { name: `remove` } }, change: { event: { name: `change` } } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="ci" host={{ onAction: (e) => void events.push([e.name, e.context]) }} />)
    const input = container.querySelector(`input`)!
    fireEvent.change(input, { target: { value: `ui` } })
    fireEvent.keyDown(input, { key: `Enter` })
    fireEvent.change(input, { target: { value: `docs,` } })
    expect(Array.from(container.querySelectorAll(`.xui-ChipInput-chipLabel`)).map((e) => e.textContent)).toEqual([`bug`, `ui`, `docs`])
    expect(events.filter(([n]) => n === `add`).map(([, c]) => (c as { value: string }).value)).toEqual([`ui`, `docs`])
    fireEvent.keyDown(input, { key: `Backspace` })
    const last = container.querySelectorAll(`[data-chip]`)[2] as HTMLElement
    expect(document.activeElement).toBe(last)
    fireEvent.keyDown(last, { key: `Backspace` })
    expect(Array.from(container.querySelectorAll(`.xui-ChipInput-chipLabel`)).map((e) => e.textContent)).toEqual([`bug`, `ui`])
    expect(events.at(-1)).toEqual([`remove`, { value: `docs` }])
  })
  it(`FileUpload: bytes to host.onUpload, metadata in the event, size and type refused`, () => {
    const uploads: [string[], unknown][] = []
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `fu`, component: `FileUpload`, props: { label: `Files`, name: `files`, multiple: true, accept: `image/*,.pdf`, maxSize: 1000 }, on: { upload: { event: { name: `up` } } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="fu" host={{ onUpload: (files, t) => void uploads.push([files.map((f) => f.name), t]), onAction: (e) => void events.push(e) }} />)
    const ok = new File([`x`], `a.png`, { type: `image/png` })
    const big = new File([`x`.repeat(2000)], `b.pdf`, { type: `application/pdf` })
    const wrong = new File([`x`], `c.zip`, { type: `application/zip` })
    fireEvent.drop(container.querySelector(`.xui-FileUpload-dropzone`)!, { dataTransfer: { files: [ok, big, wrong] } })
    expect(uploads).toEqual([[[`a.png`], { nodeId: `fu`, name: `files` }]])
    expect(events[0].context.files).toEqual([{ name: `a.png`, size: 1, type: `image/png` }])
    expect(container.querySelector(`.xui-FileUpload-error`)!.textContent).toContain(`File too large`)
    expect(container.querySelector(`.xui-FileUpload-error`)!.textContent).toContain(`Unsupported file`)
    expect(container.querySelector(`.xui-FileUpload-fileName`)!.textContent).toBe(`a.png`)
    expect(acceptsFile(`.pdf`, { name: `X.PDF`, type: `` })).toBe(true)
  })
  it(`CodeBlock: the catalog tokenizer per kind, gutter, highlighted line, copy announces`, async () => {
    vi.useFakeTimers()
    const writes: string[] = []
    Object.defineProperty(navigator, `clipboard`, { value: { writeText: (t: string) => (writes.push(t), Promise.resolve()) }, configurable: true })
    const root = tree({ id: `cb`, component: `CodeBlock`, props: { code: `const x = 1 // hi\nreturn x`, language: `ts`, lineNumbers: true, highlight: [2], title: `a.ts` } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="cb" />)
    expect(container.querySelector(`[data-r-kind="keyword"]`)!.textContent).toBe(`const`)
    expect(container.querySelector(`[data-r-kind="number"]`)!.textContent).toBe(`1`)
    expect(container.querySelector(`[data-r-kind="comment"]`)!.textContent).toBe(`// hi`)
    expect(Array.from(container.querySelectorAll(`.xui-CodeBlock-lineNumber`)).map((e) => e.textContent)).toEqual([`1`, `2`])
    expect(container.querySelector(`[data-line="2"]`)!.getAttribute(`data-xs`)).toBe(`selected`)
    await act(async () => {
      fireEvent.click(container.querySelector(`.xui-CodeBlock-copy`)!)
      await Promise.resolve()
    })
    expect(writes).toEqual([`const x = 1 // hi\nreturn x`])
    act(() => vi.advanceTimersByTime(50))
    expect(container.querySelector(`[data-xui-live="polite"]`)!.textContent).toBe(`Copied`)
  })
  it(`Toast: in the toast layer, runs its duration (paused on hover), writes open=false, fires dismiss + change`, () => {
    vi.useFakeTimers()
    const names: string[] = []
    const root = tree({ id: `to`, component: `Toast`, props: { title: `Saved`, type: `error`, open: { path: `/t` }, duration: 1000 }, on: { dismiss: { event: { name: `dismiss` } }, change: { event: { name: `change` } } } })
    const { container } = render(<ExponentialSurface root={root} data={{ t: true }} theme="neutral" id="ts" host={{ onAction: (e) => void names.push(e.name) }} />)
    const el = container.querySelector(`[data-xui-id="to"]`)!
    expect(el.closest(`[data-xui-layer="toast"]`)).not.toBeNull()
    expect(el.getAttribute(`role`)).toBe(`alert`)
    fireEvent.pointerEnter(el)
    act(() => vi.advanceTimersByTime(2000))
    expect(names).toEqual([])
    fireEvent.pointerLeave(el)
    act(() => vi.advanceTimersByTime(1100))
    expect(names).toEqual([`dismiss`, `change`])
    act(() => vi.advanceTimersByTime(500))
    expect(container.querySelector(`[data-xui-id="to"]`)).toBeNull()
  })
  it(`Table: local sort (numbers numerically, missing last), select-all, a slot cell scoped to its row, striped odd rows`, () => {
    const events: SurfaceActionEvent[] = []
    const root = tree({
      id: `tb`,
      component: `Table`,
      props: {
        columns: [
          { key: `name`, label: `Name`, sortable: true },
          { key: `runs`, label: `Runs`, type: `number`, align: `end`, sortable: true },
          { key: `status`, label: `Status`, type: `slot`, slot: `st` },
        ],
        rows: [
          { id: `a`, name: `Alice`, runs: 12, status: `On` },
          { id: `b`, name: `Bob`, runs: 3, status: `Off` },
          { id: `c`, name: `Cy`, status: `On` },
        ],
        selectable: `multiple`,
        selected: { path: `/sel` },
        striped: true,
      },
      slots: { st: { id: `pill`, component: `Text`, props: { text: { path: `status` } } } },
      on: { sort: { event: { name: `sort` } }, select: { event: { name: `select` } } },
    })
    const { container } = render(<ExponentialSurface root={root} data={{ sel: [] }} theme="neutral" id="tb" locale="en-US" host={{ onAction: (e) => void events.push(e) }} />)
    const names = () => Array.from(container.querySelectorAll(`.xui-Table-row`)).map((r) => r.getAttribute(`data-key`))
    expect(names()).toEqual([`a`, `b`, `c`])
    fireEvent.click(container.querySelectorAll(`.xui-table-sort`)[1])
    expect(names()).toEqual([`b`, `a`, `c`])
    expect(events[0].context).toEqual({ sort: { key: `runs`, direction: `asc` } })
    fireEvent.click(container.querySelectorAll(`.xui-table-sort`)[1])
    expect(names()).toEqual([`a`, `b`, `c`])
    expect(container.querySelector(`[aria-sort="descending"]`)).not.toBeNull()
    expect(Array.from(container.querySelectorAll(`[data-xui-c="Text"]`)).map((e) => e.textContent)).toEqual([`On`, `Off`, `On`])
    const rows = container.querySelectorAll(`.xui-Table-row`)
    expect(rows[1].getAttribute(`data-r-striped`)).toBe(`true`)
    expect(rows[0].getAttribute(`data-r-striped`)).toBe(`false`)
    expect(container.querySelector(`.xui-Table-cell[data-r-align="end"]`)).not.toBeNull()
    fireEvent.click(container.querySelector(`[aria-label="Select all"]`)!)
    expect(events.at(-1)!.context).toEqual({ selected: [`a`, `b`, `c`] })
    expect(sortRows([{ v: `b` }, { v: `a` }, {}], { key: `v`, direction: `asc` }, { key: `v`, label: `V` }, `en`)).toEqual([1, 0, 2])
  })
  it(`Table windows past 50 rows`, () => {
    const rows = Array.from({ length: 80 }, (_, i) => ({ id: `r${i}`, n: i }))
    const { container } = render(<ExponentialSurface root={tree({ id: `tb`, component: `Table`, props: { columns: [{ key: `n`, label: `N` }], rows } })} theme="neutral" id="tw" />)
    expect(container.querySelector(`.xui-list-window`)).not.toBeNull()
    expect(container.querySelectorAll(`.xui-Table-row`).length).toBeLessThan(80)
  })
  it(`Select searchable: a combobox field, arrows move the active option (aria-activedescendant), Enter picks, search debounced`, () => {
    vi.useFakeTimers()
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `s`, component: `Select`, props: { label: `Status`, name: `status`, searchable: true, options: [{ label: `Todo`, value: `todo` }, { label: `Doing`, value: `doing` }, { label: `Done`, value: `done` }] }, on: { change: { event: { name: `change` } }, search: { event: { name: `search` } } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="sel" host={{ onAction: (e) => void events.push(e) }} />)
    fireEvent.click(container.querySelector(`[role="combobox"]`)!)
    const field = document.querySelector(`input.xui-Select-search`) as HTMLInputElement
    fireEvent.change(field, { target: { value: `n` } })
    act(() => vi.advanceTimersByTime(200))
    expect(events.find((e) => e.name === `search`)!.context).toEqual({ query: `n` })
    expect(document.querySelectorAll(`[role="option"]`).length).toBe(2)
    expect(field.getAttribute(`aria-activedescendant`)).toBe(document.querySelectorAll(`[role="option"]`)[0].id)
    fireEvent.keyDown(field, { key: `ArrowDown` })
    expect(field.getAttribute(`aria-activedescendant`)).toBe(document.querySelectorAll(`[role="option"]`)[1].id)
    fireEvent.keyDown(field, { key: `Enter` })
    expect(events.find((e) => e.name === `change`)!.context).toEqual({ value: `done` })
  })
  it(`Chart: measured width, nice ticks, a ring per pie series, the legend with 2+ series, arrow keys move the tooltip`, () => {
    const bar = tree({ id: `c`, component: `Chart`, props: { kind: `bar`, categories: [`a`, `b`], series: [{ name: `x`, values: [3, 7] }, { name: `y`, values: [5, 1] }] } })
    const { container } = render(<ExponentialSurface root={bar} theme="neutral" id="ch" />)
    const ticks = Array.from(container.querySelectorAll(`.xui-Chart-axis text`)).map((t) => t.textContent)
    expect(ticks).toEqual(expect.arrayContaining([`0`, `2`, `4`, `6`, `8`]))
    expect(container.querySelector(`.xui-Chart-legend`)).not.toBeNull()
    const svg = container.querySelector(`svg.xui-chart-svg`)!
    expect(svg.getAttribute(`viewBox`)).toBe(`0 0 320 200`)
    fireEvent.keyDown(svg, { key: `ArrowRight` })
    expect(container.querySelector(`.xui-Chart-tooltip`)!.textContent).toContain(`x: 3`)
    fireEvent.keyDown(svg, { key: `ArrowRight` })
    expect(container.querySelector(`.xui-Chart-tooltip`)!.textContent).toContain(`y: 1`)
    cleanup()
    const pie = tree({ id: `p`, component: `Chart`, props: { kind: `donut`, categories: [`a`, `b`, `c`], series: [{ name: `in`, values: [1, 2, 3] }, { name: `out`, values: [3, 2, 1] }] } })
    const r = render(<ExponentialSurface root={pie} theme="neutral" id="pi" />)
    expect(r.container.querySelectorAll(`path[data-category]`).length).toBe(6)
  })
  it(`Image: fallback glyph on error and without a src, focal point, lazy by default`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `i`, component: `Image`, props: { src: `x.png`, alt: `X`, focalX: 0.2, focalY: 1 } })} theme="neutral" id="im" />)
    const img = container.querySelector(`img`)!
    expect(img.getAttribute(`loading`)).toBe(`lazy`)
    expect(img.style.objectPosition).toBe(`20% 100%`)
    fireEvent.error(img)
    expect(container.querySelector(`.xui-Image-fallback`)).not.toBeNull()
    expect(container.querySelector(`[data-xui-id="i"]`)!.getAttribute(`data-state`)).toBe(`error`)
  })
  it(`Image: a host media fetch that fails shows the fallback and the alt (VAPP-99)`, async () => {
    const fetchSpy = vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(null, { status: 404 }))
    try {
      const host: HostPlugin = { mediaRequest: (src) => ({ url: `https://x.test/${src}`, headers: { authorization: `Bearer t` } }) }
      const { container } = render(<ExponentialSurface root={tree({ id: `i`, component: `Image`, props: { src: `gone.png`, alt: `Gone` } })} theme="neutral" id="imf" host={host} />)
      const root = container.querySelector(`[data-xui-id="i"]`)!
      expect(root.getAttribute(`data-state`)).toBe(`loading`)
      await act(async () => {
        await new Promise((r) => setTimeout(r, 0))
      })
      expect(fetchSpy).toHaveBeenCalledWith(`https://x.test/gone.png`, { headers: { authorization: `Bearer t` } })
      expect(root.getAttribute(`data-state`)).toBe(`error`)
      expect(root.getAttribute(`aria-label`)).toBe(`Gone`)
      expect(container.querySelector(`.xui-Image-fallback`)).not.toBeNull()
      expect(container.querySelector(`img`)).toBeNull()
    } finally {
      fetchSpy.mockRestore()
    }
  })
  it(`Textarea autosize sizes the field between rows and maxRows`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `ta`, component: `Textarea`, props: { label: `Body`, name: `body`, rows: 2, maxRows: 4, autosize: true } })} theme="neutral" id="ta" />)
    const ta = container.querySelector(`textarea`)!
    expect(ta.getAttribute(`data-autosize`)).toBe(`true`)
    expect(ta.style.height).toMatch(/px$/)
  })
  it(`Menu kinds render: label, separator, checkbox (bound, writes), submenu trigger, shortcut`, () => {
    const root = tree({ id: `m`, component: `Menu`, props: { label: `View`, items: [{ kind: `label`, label: `Show` }, { kind: `checkbox`, label: `Done`, value: `done`, checked: { path: `/done` } }, { kind: `separator` }, { label: `Copy`, value: `copy`, shortcut: `⌘C` }, { kind: `submenu`, label: `More`, items: [{ label: `A`, value: `a` }] }] }, on: { select: { event: { name: `sel` } } } })
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={root} data={{ done: false }} theme="neutral" id="dm" host={{ onAction: (e) => void events.push(e) }} />)
    const trigger = container.querySelector(`.xui-Menu-trigger`)!
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: `mouse` })
    expect(document.querySelector(`.xui-Menu-label`)!.textContent).toBe(`Show`)
    expect(document.querySelector(`.xui-Menu-shortcut`)!.textContent).toBe(`⌘C`)
    expect(document.querySelector(`.xui-Menu-submenuIndicator`)).not.toBeNull()
    expect(document.querySelector(`[data-xui-overlay="Menu"]`)!.getAttribute(`data-open-on`)).toBe(`press`)
    fireEvent.click(document.querySelector(`[role="menuitemcheckbox"]`)!)
    expect(events[0].context).toEqual({ value: `done`, checked: true })
  })
})

describe(`round 3 natives (VAPP-102)`, () => {
  const open = (el: Element) => fireEvent.pointerDown(el, { button: 0, ctrlKey: false, pointerType: `mouse` })
  it(`Menu openOn press: the ONE child is the trigger (no default button, no trigger part)`, () => {
    const root = tree({ id: `m`, component: `Menu`, props: { items: [{ label: `Rename`, value: `rename` }] }, children: [{ id: `mt`, component: `Button`, props: { label: `More` } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="mp" />)
    expect(container.querySelector(`.xui-Menu-trigger`)).toBeNull()
    expect(container.querySelector(`[data-xui-id="m"]`)!.getAttribute(`data-xui-overlay-root`)).toBe(``)
    open(container.querySelector(`[data-xui-id="mt"]`)!.closest(`.xui-trigger`)!)
    expect(document.querySelector(`[data-xui-overlay="Menu"] [role="menuitem"]`)!.textContent).toBe(`Rename`)
  })
  it(`Menu openOn contextmenu: the child is the target region; right-click opens at the pointer, select fires`, () => {
    const root = tree({ id: `cm`, component: `Menu`, props: { openOn: `contextmenu`, items: [{ label: `Copy`, value: `copy` }] }, on: { select: { event: { name: `sel` } } }, children: [{ id: `area`, component: `Text`, props: { text: `Target` } }] } as unknown as NestedNode)
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="mc" host={{ onAction: (e) => void events.push(e) }} />)
    const region = container.querySelector(`[data-xui-id="cm"]`)!
    expect(region.querySelector(`[data-xui-id="area"]`)).not.toBeNull()
    expect(container.querySelector(`.xui-Menu-trigger`)).toBeNull()
    expect(document.querySelector(`[data-xui-overlay="Menu"]`)).toBeNull()
    fireEvent.contextMenu(region, { clientX: 20, clientY: 20 })
    const content = document.querySelector(`[data-xui-overlay="Menu"]`)!
    expect(content.getAttribute(`data-open-on`)).toBe(`contextmenu`)
    fireEvent.click(content.querySelector(`[role="menuitem"]`)!)
    expect(events[0].context).toEqual({ value: `copy` })
  })
  it(`Menu contextmenu: Shift+F10 on the region opens it`, () => {
    const root = tree({ id: `cm`, component: `Menu`, props: { openOn: `contextmenu`, items: [{ label: `Copy`, value: `copy` }] }, children: [{ id: `area`, component: `Text`, props: { text: `Target` } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="mk" />)
    fireEvent.keyDown(container.querySelector(`[data-xui-id="cm"]`)!, { key: `F10`, shiftKey: true })
    expect(document.querySelector(`[data-xui-overlay="Menu"]`)).not.toBeNull()
  })
  it(`a submenu's BOUND items ({path} inside a shaped array item) resolve and render like literal ones`, () => {
    const node = { id: `m`, component: `Menu`, props: { label: `Set`, items: [{ kind: `submenu`, label: `Status`, items: { path: `/statuses` } }] } } as unknown as NestedNode
    const root = tree(node)
    const data = { statuses: [{ label: `Todo`, value: `todo` }, { label: `Done`, value: `done` }] }
    // The bind pass (the renderer's and the core's reference) resolves it.
    const bound = bindTree(root, { data, scope: `` } as never)!
    expect((bound.props.items as { items: unknown }[])[0].items).toEqual(data.statuses)
    const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id="mb" />)
    open(container.querySelector(`.xui-Menu-trigger`)!)
    const sub = document.querySelector(`[data-xui-overlay="Menu"] [aria-haspopup="menu"]`)!
    expect(sub.textContent).toContain(`Status`)
    fireEvent.keyDown(sub, { key: `ArrowRight` })
    const subContent = document.querySelector(`[data-xui-overlay="Menu.sub"]`)!
    expect(Array.from(subContent.querySelectorAll(`[role="menuitem"]`)).map((e) => e.textContent)).toEqual([`Todo`, `Done`])
  })
  it(`Segmented (default segmented variant) = a radiogroup of radios; change writes the bound value`, () => {
    const root = tree({ id: `s`, component: `Segmented`, props: { items: [{ label: `List`, value: `list` }, { label: `Board`, value: `board` }], value: { path: `/view` } }, on: { change: { event: { name: `view` } } } } as unknown as NestedNode)
    const data = { view: `list` }
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id="sg" host={{ onAction: (e) => void events.push(e) }} />)
    const el = container.querySelector(`[data-xui-id="s"]`)!
    expect(el.getAttribute(`data-r-variant`)).toBe(`segmented`)
    expect(el.querySelectorAll(`.xui-Segmented-label`).length).toBe(2)
    const items = el.querySelectorAll(`.xui-Segmented-item`)
    fireEvent.click(items[1])
    expect(events[0].context).toEqual({ value: `board` })
  })
  it(`Segmented bar: role navigation, column items (icon over a label), aria-current=page, roving arrows`, () => {
    const root = tree({ id: `tb`, component: `Segmented`, props: { variant: `bar`, items: [{ label: `Inbox`, value: `inbox`, icon: `nav-inbox` }, { label: `Issues`, value: `issues`, icon: `nav-issues` }, { label: `Settings`, value: `settings`, icon: `nav-settings` }], value: `issues` }, on: { change: { event: { name: `tab` } } } } as unknown as NestedNode)
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="sb" host={{ onAction: (e) => void events.push(e) }} />)
    const nav = container.querySelector(`[data-xui-id="tb"]`)!
    expect(nav.tagName).toBe(`NAV`)
    expect(nav.getAttribute(`data-r-variant`)).toBe(`bar`)
    const items = Array.from(nav.querySelectorAll<HTMLButtonElement>(`.xui-Segmented-item`))
    expect(items.map((b) => b.getAttribute(`aria-current`))).toEqual([null, `page`, null])
    expect(items.map((b) => b.tabIndex)).toEqual([-1, 0, -1])
    // icon BEFORE label (the column puts it above)
    expect(Array.from(items[0].children).map((c) => c.className.split(` `).find((k) => k.startsWith(`xui-Segmented-`)))).toEqual([`xui-Segmented-icon`, `xui-Segmented-label`])
    expect(items[1].querySelector(`.xui-Segmented-label`)!.getAttribute(`data-xs`)).toContain(`selected`)
    items[1].focus()
    fireEvent.keyDown(items[1], { key: `ArrowRight` })
    expect(document.activeElement).toBe(items[2])
    fireEvent.keyDown(items[2], { key: `ArrowRight` })
    expect(document.activeElement).toBe(items[0])
    fireEvent.click(items[0])
    expect(events[0].context).toEqual({ value: `inbox` })
    expect(items[0].getAttribute(`aria-current`)).toBe(`page`)
  })
  it(`a Section of nested Rows: the core-computed guides land in the DOM (the kitchen sink's row-2..row-5)`, () => {
    const rows = [0, 1, 1, 2, 1, 0].map((depth, i) => ({ id: `row-${i + 1}`, component: `Row`, props: { title: `R${i + 1}`, depth } }))
    const root = tree({ id: `sec`, component: `Section`, props: { title: `Sources`, tree: true }, children: rows } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="tg" />)
    const guides = (id: string) => container.querySelector(`[data-xui-id="${id}"]`)!.querySelector(`[data-xui-c="TreeGuides"]`)
    const cols = (id: string) =>
      Array.from(guides(id)!.querySelectorAll(`.xui-tree-col`)).map((c) => ({
        elbow: c.querySelector(`[data-elbow]`) !== null,
        vertical: c.querySelector(`[data-vertical]`) !== null,
      }))
    expect(guides(`row-1`)).toBeNull()
    expect(guides(`row-6`)).toBeNull()
    // row-2: elbow at 0 + tee (a sibling follows)
    expect(cols(`row-2`)).toEqual([{ elbow: true, vertical: true }])
    expect(guides(`row-2`)!.getAttribute(`data-tee`)).toBe(`true`)
    // row-3: elbow at 0 + tee (row-5 follows at depth 1)
    expect(cols(`row-3`)).toEqual([{ elbow: true, vertical: true }])
    // row-4: pass-through at 0, elbow at 1, no tee
    expect(cols(`row-4`)).toEqual([{ elbow: false, vertical: true }, { elbow: true, vertical: false }])
    expect(guides(`row-4`)!.getAttribute(`data-tee`)).toBeNull()
    // row-5: the last child, elbow at 0, no tee
    expect(cols(`row-5`)).toEqual([{ elbow: true, vertical: false }])
    // The body is a divided List with role tree.
    expect(container.querySelector(`[data-xui-c="List"]`)!.getAttribute(`role`)).toBe(`tree`)
  })
  it(`TreeGuides geometry: 14 px columns, line at 7, elbow stub to 14, radius 3, 1 px bridge above the top`, () => {
    const root = tree({ id: `sec`, component: `Section`, children: [{ id: `a`, component: `Row`, props: { title: `A`, depth: 0 } }, { id: `b`, component: `Row`, props: { title: `B`, depth: 1 } }, { id: `c`, component: `Row`, props: { title: `C`, depth: 2 } }, { id: `d`, component: `Row`, props: { title: `D`, depth: 1 } }] } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="tgg" />)
    const g = container.querySelector(`[data-xui-id="c"]`)!.querySelector(`[data-xui-c="TreeGuides"]`)!
    const colEls = Array.from(g.querySelectorAll<HTMLElement>(`.xui-tree-col`))
    expect(colEls.map((c) => c.style.width)).toEqual([`14px`, `14px`])
    const pass = colEls[0].querySelector<HTMLElement>(`[data-vertical]`)!
    expect(pass.style.left).toBe(`6.5px`)
    expect(pass.style.width).toBe(`1px`)
    expect(pass.style.top).toBe(`-1px`)
    expect(pass.style.bottom).toBe(`0px`)
    const elbow = colEls[1].querySelector<HTMLElement>(`[data-elbow]`)!
    expect(elbow.style.left).toBe(`6.5px`)
    expect(elbow.style.width).toBe(`7.5px`)
    expect(elbow.style.top).toBe(`-1px`)
    expect(elbow.style.height).toBe(`calc(50% + 1.5px)`)
    expect(elbow.style.borderBottomLeftRadius).toBe(`3px`)
    expect(elbow.style.borderLeftWidth).toBe(`1px`)
    expect(elbow.style.borderBottomWidth).toBe(`1px`)
    expect(elbow.className).toContain(`xui-TreeGuides-line`)
  })
})

describe(`WindowedList`, () => {
  it(`unobserves items that leave and disconnects on unmount`, () => {
    const calls = { observe: 0, unobserve: 0, disconnect: 0 }
    class RO {
      observe() {
        calls.observe++
      }
      unobserve() {
        calls.unobserve++
      }
      disconnect() {
        calls.disconnect++
      }
    }
    const prev = globalThis.ResizeObserver
    ;(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = RO
    const children = Array.from({ length: 60 }, (_, i) => ({ id: `r${i}`, component: `Text`, props: { text: `row ${i}` } }))
    const { unmount } = render(<ExponentialSurface root={tree({ id: `l`, component: `List`, children })} theme="neutral" id="wl" />)
    expect(calls.observe).toBeGreaterThan(0)
    unmount()
    expect(calls.disconnect).toBeGreaterThan(0)
    ;(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = prev
  })
})
