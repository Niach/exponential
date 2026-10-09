// Round 1 renderer-hardening FIX lane (review of the React lane): one test
// per finding — the event context merge (payload wins, like the Rust core),
// `required` = the contract's, the viewport (not content) height for
// height/orientation conditions, a resize that changes nothing re-renders
// nothing, memoized nodes, template instance ids, dynamic style values that
// resolve to nothing, the Table's roving tab stop and literal-row slot
// edits, RTL (chart mirror, calendar keys by the SURFACE direction, logical
// CSS), localized sizes + phrases, stale optionSource answers, an honest
// Copy, locale digits, the calendar's tab stop. The Chromium half (windowing
// an unbounded List, keyboard through a windowed Table, two hover Popovers) is
// browser/hardening.test.ts.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { useState } from "react"
import { BIND_FUNCTIONS, CORE_CATALOG_ID, reduceNested } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import { ExponentialSurface, surfaceLayout } from "./surface"
import { CLIENT_FUNCTIONS, hostPath, LITERAL_ROWS_ROOT } from "./data"
import { BASE_CSS } from "./base-css"
import { defineReactExtension } from "./extensions"
import { calendarStop, monthGrid } from "./natives/dates"
import { parseLocaleNumber } from "./natives/inputs"
import { formatFileSize, phrase } from "./natives/shared"
import type { ExtensionComponentProps, HostPlugin, SurfaceActionEvent, SurfaceInputEvent } from "./host"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
/** Root + the lifted templates (round 2 §4) as surface props. */
const reduced = (node: NestedNode) => {
  const r = reduceNested(node, { catalogId: CORE_CATALOG_ID })
  return { root: r.root, templates: r.templates }
}

describe(`event context: the component payload wins a clashing key (Rust core fire())`, () => {
  it(`a bound Input's change carries the TYPED value, not the author's stale read of the same path`, () => {
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `q`, component: `Input`, props: { label: `Q`, value: { path: `/q` } }, on: { change: { event: { name: `typed`, context: { value: { path: `/q` }, extra: `kept` } } } } })
    const { container } = render(<ExponentialSurface root={root} data={{ q: `` }} theme="neutral" id="ev" host={{ onAction: (e) => void events.push(e) }} />)
    fireEvent.change(container.querySelector(`input`)!, { target: { value: `abc` } })
    expect(events[0].context).toMatchObject({ value: `abc`, extra: `kept` })
  })
})

describe(`required = the contract's (dynamic.ts LOGIC_FUNCTIONS, Rust data.rs)`, () => {
  it(`matches the TS reference on every truthiness edge (false fails, 0 passes)`, () => {
    for (const value of [false, true, 0, 1, ``, ` `, `x`, null, undefined, [], [0], {}, NaN]) {
      expect(CLIENT_FUNCTIONS.required({ value }, { data: {} }), JSON.stringify(value) ?? `undefined`).toBe(BIND_FUNCTIONS.required({ value }))
    }
    expect(CLIENT_FUNCTIONS.required({ value: false }, { data: {} })).toBe(false)
  })
  it(`a Checkbox checked=false fails its required check`, () => {
    const root = tree({ id: `agree`, component: `Checkbox`, props: { label: `Agree`, name: `agree`, checked: { path: `/agree` }, checks: [{ condition: { call: `required`, args: { value: { path: `/agree` } } }, message: `Please agree` }] } })
    const { container } = render(<ExponentialSurface root={root} data={{ agree: false }} theme="neutral" id="rq" />)
    fireEvent.click(container.querySelector(`[role="checkbox"]`)!)
    fireEvent.click(container.querySelector(`[role="checkbox"]`)!)
    expect(container.textContent).toContain(`Please agree`)
  })
})

describe(`height/orientation conditions read the VIEWPORT height, never the content`, () => {
  const inputs = { breakpoints: { sm: 640, md: 768 }, queries: [`@media (orientation: landscape)`, `@media (orientation: portrait)`, `@media (min-height: 600px)`], hover: true, reducedMotion: false }
  it(`unknown height before layout = landscape (contract §2)`, () => {
    expect(surfaceLayout({ width: 1280, height: undefined, measured: false }, { ...inputs, viewportHeight: undefined }).xq).toBe(`orientation-landscape`)
    expect(surfaceLayout({ width: 300, height: undefined, measured: true }, { ...inputs, viewportHeight: undefined }).xq).toBe(`orientation-landscape`)
  })
  it(`an explicit surface height (inline style) is a viewport: 1280 × 900 is landscape and min-height holds`, () => {
    expect(surfaceLayout({ width: 1280, height: 900, measured: true }, { ...inputs, viewportHeight: undefined }).xq).toBe(`orientation-landscape min-height-600px`)
  })
  it(`the viewportHeight prop wins (a host panel)`, () => {
    const out = surfaceLayout({ width: 400, height: undefined, measured: true }, { ...inputs, viewportHeight: 500 })
    expect(out).toEqual({ breakpoint: null, xq: `orientation-portrait` })
    const root = tree({ id: `n`, component: `Box`, style: { "@media (orientation: portrait)": { gap: 4 } } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" width={300} viewportHeight={800} id="vh" />)
    expect((container.firstElementChild as HTMLElement).dataset.xq).toBe(`orientation-portrait`)
  })
})

describe(`re-render economy`, () => {
  it(`a parent re-render with no host and unchanged props does not re-render the nodes`, () => {
    let paints = 0
    const Counting = ({ rootProps }: ExtensionComponentProps) => {
      paints++
      return <span {...(rootProps as Record<string, unknown>)}>counted</span>
    }
    const extensions = [defineReactExtension({ components: { Text: Counting } })]
    const root = tree({ id: `t`, component: `Text`, props: { text: `x` } })
    let bump: () => void = () => {}
    function Parent() {
      const [n, setN] = useState(0)
      bump = () => setN(n + 1)
      return (
        <div data-n={n}>
          <ExponentialSurface root={root} theme="neutral" id="memo" extensions={extensions} />
        </div>
      )
    }
    render(<Parent />)
    const before = paints
    act(() => bump())
    act(() => bump())
    expect(paints).toBe(before)
  })
  it(`a resize that changes neither the breakpoint nor data-xq re-renders nothing`, () => {
    const observers: (() => void)[] = []
    const RealRO = globalThis.ResizeObserver
    globalThis.ResizeObserver = class {
      cb: () => void
      constructor(cb: () => void) {
        this.cb = cb
        observers.push(() => this.cb())
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver
    let width = 900
    const rect = vi.spyOn(HTMLElement.prototype, `getBoundingClientRect`).mockImplementation(function (this: HTMLElement) {
      return this.classList.contains(`xui-surface`) ? ({ width, height: 400, top: 0, left: 0, right: width, bottom: 400, x: 0, y: 0, toJSON: () => ({}) } as DOMRect) : ({ width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect)
    })
    try {
      let paints = 0
      const Counting = ({ rootProps }: ExtensionComponentProps) => {
        paints++
        return <span {...(rootProps as Record<string, unknown>)}>counted</span>
      }
      const extensions = [defineReactExtension({ components: { Text: Counting } })]
      const root = tree({ id: `t`, component: `Text`, props: { text: `x` } })
      render(<ExponentialSurface root={root} theme="neutral" id="rs" extensions={extensions} />)
      const settled = paints
      for (const w of [901, 950, 1000, 1010]) {
        width = w
        act(() => observers.forEach((o) => o()))
      }
      expect(paints).toBe(settled)
      width = 500 // crosses md (768) and sm (640): one re-render
      act(() => observers.forEach((o) => o()))
      expect(paints).toBeGreaterThan(settled)
    } finally {
      rect.mockRestore()
      globalThis.ResizeObserver = RealRO
    }
  })
})

describe(`template instance ids`, () => {
  it(`duplicate, empty and missing keys fall back to #<index>; ids stay unique`, () => {
    const nested = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `l`, component: `List`, template: { component: `row`, path: `/rows`, key: `id` }, children: [] },
        { id: `defs`, component: `Box`, visible: false, children: [{ id: `row`, component: `Text`, props: { text: { path: `name` } } }] },
      ],
    } as unknown as NestedNode
    const rows = [{ id: 1, name: `a` }, { id: 1, name: `b` }, { id: ``, name: `c` }, { name: `d` }, { id: `#1`, name: `e` }]
    const { container } = render(<ExponentialSurface {...reduced(nested)} data={{ rows }} theme="neutral" id="tk" />)
    const ids = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-c="Text"]`)).map((e) => e.dataset.xuiId)
    expect(ids).toEqual([`row.1`, `row.#1`, `row.#2`, `row.#3`, `row.#4`])
    expect(Array.from(container.querySelectorAll(`[data-xui-c="Text"]`)).map((e) => e.textContent)).toEqual([`a`, `b`, `c`, `d`, `e`])
  })
  it(`nested templates ACCUMULATE suffixes (unique across outer items); duplicates never collide`, () => {
    const nested = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `outer`, component: `List`, template: { component: `orow`, path: `/groups`, key: `id` }, children: [] },
        { id: `defs`, component: `Box`, visible: false, children: [
          { id: `orow`, component: `Box`, children: [{ id: `inner`, component: `List`, template: { component: `cell`, path: `items` }, children: [] }] },
          { id: `cell`, component: `Text`, props: { text: { path: `t` } } },
        ] },
      ],
    } as unknown as NestedNode
    const data = { groups: [{ id: `a`, items: [{ t: `a0` }, { t: `a1` }] }, { id: `a`, items: [{ t: `b0` }] }, { id: null, items: [] }] }
    const { container } = render(<ExponentialSurface {...reduced(nested)} data={data} theme="neutral" id="nt" />)
    const ids = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-c="Text"]`)).map((e) => e.dataset.xuiId)
    expect(ids).toEqual([`cell.a.0`, `cell.a.1`, `cell.#1.0`])
    const all = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-id]`)).map((e) => e.dataset.xuiId!)
    expect(all.filter((id) => id.startsWith(`orow`))).toEqual([`orow.a`, `orow.#1`, `orow.#2`])
    expect(new Set(all).size).toBe(all.length)
  })
})

describe(`dynamic style values`, () => {
  it(`a value that resolves to nothing sets the variable to initial (no inheriting an ancestor's --xd-n)`, () => {
    // An expander-made dynamic style (authors cannot write one): the parent's
    // width resolves, the child's does not.
    const root = {
      id: `parent`,
      component: `Box`,
      props: {},
      style: { width: { call: `percent`, args: { value: { path: `/a` }, max: 100 } } },
      children: [{ id: `child`, component: `Box`, props: {}, style: { width: { path: `/missing` } }, children: [] }],
    }
    const { container } = render(<ExponentialSurface root={root} data={{ a: 40 }} theme="neutral" id="dy" />)
    expect((container.querySelector(`[data-xui-id="parent"]`) as HTMLElement).style.getPropertyValue(`--xd-0`)).toBe(`40%`)
    expect((container.querySelector(`[data-xui-id="child"]`) as HTMLElement).style.getPropertyValue(`--xd-0`)).toBe(`initial`)
  })
})

describe(`Table keyboard and literal-row slots`, () => {
  const columns = [{ key: `name`, label: `Name` }, { key: `done`, label: `Done`, type: `slot`, slot: `ok` }]
  it(`one roving tab stop: the first row, else the first selected; arrows/Home/End move it by index`, () => {
    const rows = [{ id: `a`, name: `A` }, { id: `b`, name: `B` }, { id: `c`, name: `C` }]
    const root = tree({ id: `t`, component: `Table`, props: { columns: [{ key: `name`, label: `Name` }], rows, selectable: `single`, selected: [`b`] } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="rt" />)
    const stops = () => Array.from(container.querySelectorAll<HTMLElement>(`[role="row"][tabindex="0"]`)).map((r) => r.dataset.key)
    expect(stops()).toEqual([`b`])
    const row = (k: string) => container.querySelector<HTMLElement>(`[role="row"][data-key="${k}"]`)!
    row(`b`).focus()
    fireEvent.keyDown(row(`b`), { key: `ArrowDown` })
    expect(document.activeElement).toBe(row(`c`))
    expect(stops()).toEqual([`c`])
    fireEvent.keyDown(row(`c`), { key: `Home` })
    expect(document.activeElement).toBe(row(`a`))
    fireEvent.keyDown(row(`a`), { key: `End` })
    expect(document.activeElement).toBe(row(`c`))
    fireEvent.keyDown(row(`c`), { key: `ArrowDown` })
    expect(document.activeElement).toBe(row(`c`))
  })
  it(`a slot edit over LITERAL rows stays (table-local), never reaches the data model or the host as a path`, () => {
    const inputs: SurfaceInputEvent[] = []
    const datas: unknown[] = []
    const host: HostPlugin = { onInput: (e) => void inputs.push(e) }
    const root = tree({ id: `t`, component: `Table`, props: { columns, rows: [{ id: `a`, name: `A`, done: false }, { id: `b`, name: `B`, done: true }] }, slots: { ok: { id: `ok`, component: `Checkbox`, props: { label: `Done`, checked: { path: `done` } } } } })
    const { container } = render(<ExponentialSurface root={root} data={{}} theme="neutral" id="lr" host={host} />)
    const box = () => container.querySelector<HTMLElement>(`[data-xui-id="ok.a"] [role="checkbox"]`)!
    expect(box().getAttribute(`data-state`)).toBe(`unchecked`)
    fireEvent.click(box())
    expect(box().getAttribute(`data-state`)).toBe(`checked`)
    expect(container.querySelector<HTMLElement>(`[data-xui-id="ok.b"] [role="checkbox"]`)!.getAttribute(`data-state`)).toBe(`checked`)
    for (const e of inputs) expect(e.path ?? ``).not.toContain(LITERAL_ROWS_ROOT)
    void datas
    expect(hostPath(`${LITERAL_ROWS_ROOT}/t/0/done`)).toBeUndefined()
    expect(hostPath(`/rows/0/done`)).toBe(`/rows/0/done`)
  })
  it(`slot ids APPEND the row key to the outer instance`, () => {
    const nested = {
      id: `root`,
      component: `Box`,
      children: [
        { id: `outer`, component: `List`, template: { component: `card`, path: `/teams` }, children: [] },
        { id: `defs`, component: `Box`, visible: false, children: [
          { id: `card`, component: `Table`, props: { columns: [{ key: `n`, label: `N`, type: `slot`, slot: `c` }], rows: { path: `rows` } }, slots: { c: { id: `cell`, component: `Text`, props: { text: { path: `n` } } } } },
        ] },
      ],
    } as unknown as NestedNode
    const { container } = render(<ExponentialSurface {...reduced(nested)} data={{ teams: [{ rows: [{ id: `x`, n: `1` }] }, { rows: [{ id: `x`, n: `2` }] }] }} theme="neutral" id="si" />)
    const ids = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-c="Text"]`)).map((e) => e.dataset.xuiId)
    expect(ids).toEqual([`cell.0.x`, `cell.1.x`])
  })
})

describe(`RTL`, () => {
  it(`Chart mirrors its cartesian kinds: the first category sits on the RIGHT, ArrowRight still moves the highlight right`, () => {
    const root = tree({ id: `c`, component: `Chart`, props: { kind: `bar`, categories: [`first`, `last`], series: [{ name: `x`, values: [3, 7] }] } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="cr" direction="rtl" />)
    const x = (label: string) => Number(Array.from(container.querySelectorAll(`.xui-Chart-axis text`)).find((t) => t.textContent === label)!.getAttribute(`x`))
    expect(x(`first`)).toBeGreaterThan(x(`last`))
    expect(container.querySelector(`[data-mirrored="true"]`)).not.toBeNull()
    const svg = container.querySelector(`svg.xui-chart-svg`)!
    fireEvent.keyDown(svg, { key: `ArrowRight` })
    expect(container.querySelector(`.xui-Chart-tooltip`)!.textContent).toContain(`last`)
  })
  it(`the calendar flips arrows by the SURFACE direction, not document.dir`, () => {
    document.dir = `rtl`
    try {
      const root = tree({ id: `d`, component: `DatePicker`, props: { label: `When`, value: `2026-10-14` } })
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="dl" direction="ltr" />)
      fireEvent.click(container.querySelector(`.xui-DatePicker-trigger`)!)
      const day = document.querySelector<HTMLElement>(`[data-date="2026-10-14"]`)!
      day.focus()
      fireEvent.keyDown(day, { key: `ArrowRight` })
      expect((document.activeElement as HTMLElement).dataset.date).toBe(`2026-10-15`)
    } finally {
      document.dir = ``
    }
  })
  it(`base CSS uses logical sides for the Dialog close and the Select check, and mirrors the Switch thumb`, () => {
    expect(BASE_CSS).toContain(`.xui-Dialog-close{position:absolute;top:var(--xui-spacing-sm);inset-inline-end:`)
    expect(BASE_CSS).toContain(`.xui-Select-check{display:inline-flex;margin-inline-start:auto`)
    expect(BASE_CSS).toContain(`.xui-Switch-thumb:dir(rtl){transform:translateX(calc(-1 * var(--xui-switch-travel,12px)))}`)
  })
})

describe(`locale`, () => {
  it(`file sizes use the locale's unit names and digits`, () => {
    expect(formatFileSize(512, `en-US`)).toBe(`512 byte`)
    expect(formatFileSize(1536, `en-US`)).toBe(`1.5 kB`)
    expect(formatFileSize(1536, `de-DE`)).toBe(`1,5 kB`)
    expect(formatFileSize(3 * 1024 * 1024, `en-US`)).toBe(`3 MB`)
    expect(formatFileSize(1536, `ar-EG`)).toMatch(/١٫٥/)
  })
  it(`phrases come from the string table when it has the id (word order is the translation's), else the fallback`, () => {
    const ctx = (strings: Record<string, string>) => ({ strings, t: (id: string, p?: Record<string, unknown>) => (strings[id] ?? id).replace(/\{(\w+)\}/g, (_, k: string) => String(p?.[k] ?? ``)) })
    expect(phrase(ctx({ removeItem: `{name} entfernen` }), `removeItem`, { name: `a.txt` }, () => `fallback`)).toBe(`a.txt entfernen`)
    expect(phrase(ctx({}), `removeItem`, { name: `a.txt` }, () => `Remove a.txt`)).toBe(`Remove a.txt`)
  })
  it(`a NumberField parses the locale's own digits (ar-EG) as well as ASCII`, () => {
    expect(parseLocaleNumber(`٥`, `ar-EG`)).toBe(5)
    expect(parseLocaleNumber(new Intl.NumberFormat(`ar-EG`).format(-1234.5), `ar-EG`)).toBe(-1234.5)
    expect(parseLocaleNumber(`12`, `ar-EG`)).toBe(12)
    expect(parseLocaleNumber(`1.234,5`, `de-DE`)).toBe(1234.5)
  })
})

describe(`async and platform honesty`, () => {
  it(`Select optionSource: an older answer resolving LAST is dropped`, async () => {
    vi.useFakeTimers()
    const pending: { query: string; resolve: (o: { label: string; value: string }[]) => void }[] = []
    const host: HostPlugin = { optionSource: (_source, query) => new Promise((resolve) => void pending.push({ query, resolve })) }
    const root = tree({ id: `s`, component: `Select`, props: { label: `Who`, searchable: true, source: `people`, options: [] } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="os" host={host} />)
    fireEvent.click(container.querySelector(`[role="combobox"]`)!)
    const input = container.querySelector(`input[role="combobox"]`) ?? document.querySelector(`input[role="combobox"]`)!
    fireEvent.change(input, { target: { value: `ab` } })
    act(() => vi.advanceTimersByTime(400))
    fireEvent.change(input, { target: { value: `abc` } })
    act(() => vi.advanceTimersByTime(400))
    const ab = pending.find((p) => p.query === `ab`)!
    const abc = pending.find((p) => p.query === `abc`)!
    await act(async () => {
      abc.resolve([{ label: `ABC result`, value: `abc` }])
      await Promise.resolve()
    })
    await act(async () => {
      ab.resolve([{ label: `AB result`, value: `ab` }])
      await Promise.resolve()
    })
    expect(document.body.textContent).toContain(`ABC result`)
    expect(document.body.textContent).not.toContain(`AB result`)
  })
  it(`CodeBlock Copy without a clipboard (insecure origin) or on a refusal does not claim Copied`, async () => {
    vi.useFakeTimers()
    const root = tree({ id: `cb`, component: `CodeBlock`, props: { code: `x`, language: `ts` } })
    for (const clipboard of [undefined, { writeText: () => Promise.reject(new Error(`denied`)) }]) {
      Object.defineProperty(navigator, `clipboard`, { value: clipboard, configurable: true })
      const { container, unmount } = render(<ExponentialSurface root={root} theme="neutral" id="cp" />)
      await act(async () => {
        fireEvent.click(container.querySelector(`.xui-CodeBlock-copy`)!)
        await Promise.resolve()
        await Promise.resolve()
      })
      act(() => vi.advanceTimersByTime(100))
      expect(container.querySelector(`.xui-CodeBlock-copy`)!.getAttribute(`aria-label`)).toBe(`Copy`)
      expect(container.querySelector(`[data-xui-live="polite"]`)!.textContent).toBe(``)
      unmount()
    }
  })
})

describe(`calendar tab stop`, () => {
  const disabledBefore = (min: Date) => (d: Date) => d < min
  it(`a disabled focused day never holds the stop: the nearest enabled in-month day does`, () => {
    const view = new Date(2026, 9, 1)
    const days = monthGrid(view, 1)
    const stop = calendarStop(days, view, new Date(2026, 9, 8), disabledBefore(new Date(2026, 9, 20)))
    expect(stop && stop.getDate()).toBe(20)
    expect(calendarStop(days, view, new Date(2026, 9, 25), disabledBefore(new Date(2026, 9, 20)))!.getDate()).toBe(25)
    expect(calendarStop(days, view, new Date(2026, 9, 25), () => true)).toBeNull()
  })
  it(`min after today: the grid still has an enabled tab stop; the month buttons carry it into the new view`, () => {
    // min = the 10th, three months after today's month (a short walk).
    const now = new Date()
    const minDate = new Date(now.getFullYear(), now.getMonth() + 3, 10)
    const min = `${minDate.getFullYear()}-${String(minDate.getMonth() + 1).padStart(2, `0`)}-10`
    const root = tree({ id: `d`, component: `DatePicker`, props: { label: `When`, min } })
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="ct" />)
    fireEvent.click(container.querySelector(`.xui-DatePicker-trigger`)!)
    const stop = () => document.querySelector<HTMLButtonElement>(`.xui-calendar-grid button[tabindex="0"]`)
    // today's month (before min): every day disabled → no stop in it
    fireEvent.click(document.querySelector(`[aria-label="Next month"]`)!)
    const months = Array.from({ length: 6 }).findIndex(() => {
      if (stop()) return true
      fireEvent.click(document.querySelector(`[aria-label="Next month"]`)!)
      return false
    })
    expect(months).toBe(2)
    expect(stop()!.disabled).toBe(false)
    expect(stop()!.dataset.date).toBe(min)
    fireEvent.click(document.querySelector(`[aria-label="Next month"]`)!)
    expect(stop()).not.toBeNull()
    expect(stop()!.dataset.date!.slice(0, 7)).toMatch(/^\d{4}-\d{2}$/)
  })
})
