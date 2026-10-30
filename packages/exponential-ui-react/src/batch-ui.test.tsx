// EXP-1259 (the UI batch): the SearchList (filter, highlighted row, keys,
// pick, count, the core's part ids), a searchable Menu submenu, the Tabs
// close glyph (`closable: hover` borrows its tab's hover) and + button, the
// pending Button (the 150 ms delay, `pendingLabel`) and Skeleton rows.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, PENDING_DELAY_MS, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import { BASE_CSS } from "@exponential_app/eui/web"
import { compileTheme } from "@exponential_app/eui/web"
import { builtinTheme } from "@exponential_app/eui"
import type { SurfaceActionEvent } from "./host"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

const items = [
  { label: `Ship the search list`, value: `a`, identifier: `EXP-12`, icon: `status-in-progress` },
  { label: `Spinner sizes`, value: `b`, identifier: `EXP-14` },
  { label: `Spin the mark`, value: `c`, identifier: `EXP-15` },
]

describe(`SearchList`, () => {
  it(`lays out the core's parts, filters, moves the highlight with the keys and picks with Enter`, () => {
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `sl`, component: `SearchList`, props: { items, value: { path: `/picked` } }, on: { select: { event: { name: `pick` } } } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} data={{ picked: `` }} theme="neutral" id="sl" host={{ onAction: (e) => void events.push(e) }} />)
    for (const id of [`sl.head`, `sl.search`, `sl.body`, `sl.list`, `sl.item.0`, `sl.icon.0`, `sl.identifier.0`, `sl.label.0`]) expect(container.querySelector(`[data-xui-id="${id}"]`), id).not.toBeNull()
    expect(container.querySelector(`[data-xui-id="sl.count"]`)).toBeNull()
    const input = container.querySelector<HTMLInputElement>(`[data-xui-id="sl.search"] input`)!
    expect(input.placeholder).toBe(`Search…`)
    fireEvent.change(input, { target: { value: `spin` } })
    expect(container.querySelector(`[data-xui-id="sl.item.0"]`)).toBeNull()
    expect(container.querySelector(`[data-xui-id="sl.count"]`)!.textContent).toBe(`2`)
    expect(container.querySelector(`[data-xui-id="sl.item.1"]`)!.getAttribute(`data-xs`)).toContain(`selected`)
    fireEvent.keyDown(input, { key: `ArrowDown` })
    expect(container.querySelector(`[data-xui-id="sl.item.2"]`)!.getAttribute(`data-xs`)).toContain(`selected`)
    expect(input.getAttribute(`aria-activedescendant`)).toBe(container.querySelector(`[data-xui-id="sl.item.2"]`)!.id)
    fireEvent.keyDown(input, { key: `ArrowDown` })
    expect(container.querySelector(`[data-xui-id="sl.item.2"]`)!.getAttribute(`data-xs`)).toContain(`selected`)
    fireEvent.keyDown(input, { key: `Enter` })
    expect(events.find((e) => e.name === `pick`)!.context).toEqual({ value: `c` })
    expect(container.querySelector(`[data-xui-id="sl.check.2"]`)).not.toBeNull()
  })

  it(`a host that searches (on.search) ranks the rows itself and hears the query after the debounce`, () => {
    vi.useFakeTimers()
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `sl`, component: `SearchList`, props: { items }, on: { search: { event: { name: `q` } } } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="sq" host={{ onAction: (e) => void events.push(e) }} />)
    fireEvent.change(container.querySelector(`[data-xui-id="sl.search"] input`)!, { target: { value: `zzz` } })
    expect(container.querySelector(`[data-xui-id="sl.item.0"]`)).not.toBeNull()
    act(() => void vi.advanceTimersByTime(200))
    expect(events.find((e) => e.name === `q`)!.context).toEqual({ query: `zzz` })
  })

  it(`shows the empty line when nothing matches`, () => {
    const root = tree({ id: `sl`, component: `SearchList`, props: { items, emptyText: `Nothing` } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="se" />)
    fireEvent.change(container.querySelector(`[data-xui-id="sl.search"] input`)!, { target: { value: `qqq` } })
    expect(container.querySelector(`[data-xui-id="sl.empty"]`)!.textContent).toBe(`Nothing`)
  })
})

describe(`a searchable Menu submenu`, () => {
  it(`filters its entries in a search list and picks with Enter`, () => {
    const events: SurfaceActionEvent[] = []
    const root = tree({ id: `m`, component: `Menu`, props: { label: `Add`, items: [{ kind: `submenu`, label: `Implement issue`, searchable: true, placeholder: `Search issues`, items: [{ label: `Ship it`, value: `a` }, { label: `Fix tabs`, value: `b` }, { label: `Fix search`, value: `c` }] }] }, on: { select: { event: { name: `pick` } } } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="ms" host={{ onAction: (e) => void events.push(e) }} />)
    fireEvent.pointerDown(container.querySelector(`.xui-Menu-trigger`)!, { button: 0, ctrlKey: false, pointerType: `mouse` })
    fireEvent.keyDown(document.querySelector(`[data-xui-overlay="Menu"] [aria-haspopup="menu"]`)!, { key: `ArrowRight` })
    const sub = document.querySelector(`[data-xui-overlay="Menu.sub"]`)!
    expect(sub.getAttribute(`data-searchable`)).toBe(`true`)
    const input = sub.querySelector<HTMLInputElement>(`input`)!
    expect(input.placeholder).toBe(`Search issues`)
    fireEvent.change(input, { target: { value: `fix` } })
    expect(Array.from(sub.querySelectorAll(`[role="option"]`)).map((o) => o.textContent)).toEqual([`Fix tabs`, `Fix search`])
    fireEvent.keyDown(input, { key: `ArrowDown` })
    fireEvent.keyDown(input, { key: `Enter` })
    expect(events.find((e) => e.name === `pick`)!.context).toEqual({ value: `c` })
  })
})

describe(`Tabs closable + addable`, () => {
  const node = { id: `t`, component: `Tabs`, props: { tabs: [{ label: `One`, value: `1` }, { label: `Two`, value: `2` }], closable: `hover`, addable: true }, on: { close: { event: { name: `close` } }, new: { event: { name: `new` } } }, children: [{ id: `p1`, component: `Text`, props: { text: `1` } }, { id: `p2`, component: `Text`, props: { text: `2` } }] } as unknown as NestedNode
  it(`renders a close glyph per tab and the + button, both firing their events`, () => {
    const events: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={tree(node)} theme="neutral" id="tc" host={{ onAction: (e) => void events.push(e) }} />)
    const close = container.querySelector(`[data-xui-id="t.close.1"]`)!
    expect(close.getAttribute(`aria-label`)).toBe(`Close Two`)
    fireEvent.click(close)
    expect(events.find((e) => e.name === `close`)!.context).toEqual({ value: `2` })
    fireEvent.click(container.querySelector(`[data-xui-id="t.new"]`)!)
    expect(events.some((e) => e.name === `new`)).toBe(true)
  })
  it(`a hover close glyph borrows its tab's hover`, () => {
    const { container } = render(<ExponentialSurface root={tree(node)} theme="neutral" id="th" />)
    const close = container.querySelector(`[data-xui-id="t.close.0"]`)!
    expect(close.getAttribute(`data-r-closable`)).toBe(`hover`)
    expect(close.getAttribute(`data-xs`) ?? ``).not.toContain(`hover`)
    fireEvent.pointerEnter(container.querySelector(`[data-xui-id="t.tab.0"]`)!)
    expect(close.getAttribute(`data-xs`)).toContain(`hover`)
    expect(container.querySelector(`[data-xui-id="t.close.1"]`)!.getAttribute(`data-xs`) ?? ``).not.toContain(`hover`)
  })
  it(`the theme hides a hover close glyph until hovered`, () => {
    const css = compileTheme(builtinTheme(`neutral`)!).css
    expect(css).toMatch(/xui-Tabs-close[^{]*data-r-closable="hover"[^{]*\{[^}]*opacity:0/)
  })
})

describe(`the pending Button`, () => {
  it(`refuses presses at once, shows the spinner and pendingLabel only after the delay`, () => {
    vi.useFakeTimers()
    const root = tree({ id: `b`, component: `Button`, props: { label: `Start`, loading: true, pendingLabel: `Starting` } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="pb" />)
    const button = container.querySelector<HTMLButtonElement>(`[data-xui-id="b"]`)!
    expect(button.disabled).toBe(true)
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    expect(button.textContent).toBe(`Start`)
    expect(button.querySelector(`.xui-Button-spinner`)).toBeNull()
    act(() => void vi.advanceTimersByTime(PENDING_DELAY_MS + 1))
    expect(button.textContent).toBe(`Starting`)
    expect(button.querySelector(`.xui-Button-spinner`)).not.toBeNull()
    expect(button.getAttribute(`data-pending`)).toBe(`true`)
  })
})

describe(`Skeleton rows`, () => {
  it(`rows of a dot and two bars, in the core's part ids`, () => {
    const root = tree({ id: `sk`, component: `Skeleton`, props: { rows: 3, rowHeight: 44 } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="sk" />)
    for (const id of [`sk.row.0`, `sk.dot.0`, `sk.bar.0.0`, `sk.bar.0.1`, `sk.row.2`]) expect(container.querySelector(`[data-xui-id="${id}"]`), id).not.toBeNull()
    expect((container.querySelector(`[data-xui-id="sk.row.1"]`) as HTMLElement).style.height).toBe(`44px`)
    const none = tree({ id: `nd`, component: `Skeleton`, props: { rows: 1, dot: false } } as unknown as NestedNode)
    cleanup()
    const { container: c2 } = render(<ExponentialSurface root={none} theme="neutral" id="nd" />)
    expect(c2.querySelector(`[data-xui-id="nd.dot.0"]`)).toBeNull()
  })
  it(`pulses on the feedback-states timing and holds still under reduce motion`, () => {
    expect(BASE_CSS).toContain(`@keyframes xui-skeleton{0%,100%{opacity:0.45}50%{opacity:0.9}}`)
    expect(BASE_CSS).toContain(`animation:xui-skeleton 1600ms ease-in-out infinite`)
    expect(BASE_CSS).toMatch(/xui-Skeleton-bar\{animation:none !important;opacity:0\.6 !important\}/)
  })
})

describe(`motion tokens`, () => {
  it(`the theme carries the scale tokens; buttons press to it and popovers enter from it`, () => {
    const css = compileTheme(builtinTheme(`exponential`)!).css
    expect(css).toContain(`--xui-scale-press:0.97`)
    expect(css).toContain(`--xui-scale-popoverFrom:0.96`)
    expect(BASE_CSS).toContain(`scale:var(--xui-scale-press,1)`)
    expect(BASE_CSS).toContain(`@keyframes xui-pop-in{from{opacity:0;scale:var(--xui-scale-popoverFrom,.96)}}`)
    expect(BASE_CSS).toContain(`@keyframes xui-pop-out{to{opacity:0}}`)
  })
})
