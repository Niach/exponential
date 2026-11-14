// EXP-1832: a `reorderable` List in Solid (React's list-reorder.test.tsx), replaying fixtures/list-reorder.json
// `dropSlot` through the real DOM (jsdom has no layout: each row's box comes
// from the case's `rows`). The items carry the web controller's DOM contract
// (`data-reorder-*`); a drag fires `move` with the case's context.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested, TREE_GUIDE_COLUMN } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/list-reorder.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

interface Row {
  key: string
  top: number
  height: number
  depth?: number
  group?: boolean
}
interface Case {
  name: string
  rows: Row[]
  drag: string
  at: number
  maxDepth: number
  move: Record<string, string> | null
}

/** A pointer event jsdom builds the same everywhere (a MouseEvent with the
 *  pointer fields), dispatched at `target`. */
function pointer(target: EventTarget, type: string, x: number, y: number) {
  const e = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(e, `pointerId`, { value: 1 })
  Object.defineProperty(e, `pointerType`, { value: `mouse` })
  target.dispatchEvent(e)
}

function mount(rows: Row[], maxDepth: number, extra: Record<string, unknown> = {}) {
  const actions: SurfaceActionEvent[] = []
  const r = reduceNested(
    {
      id: `root`,
      component: `Box`,
      children: [
        { id: `l`, component: `List`, props: { reorderable: true, maxDepth, ...extra }, template: { component: `row`, path: `/rows`, key: `key` }, on: { move: { event: { name: `move` } } }, children: [] },
        { id: `defs`, component: `Box`, visible: false, children: [{ id: `row`, component: `Text`, props: { text: { "@path": `key` } } }] },
      ],
    } as unknown as NestedNode,
    { catalogId: CORE_CATALOG_ID }
  )
  const { container } = render(() => <ExponentialSurface root={r.root} templates={r.templates} data={{ rows }} theme="neutral" id="lr" host={{ onAction: (e) => void actions.push(e) }} />)
  const el = (key: string) => container.querySelector<HTMLElement>(`[data-reorder-key="${key}"]`)
  for (const row of rows) {
    const node = el(row.key)
    if (node) node.getBoundingClientRect = () => ({ left: 0, top: row.top, right: 300, bottom: row.top + row.height, width: 300, height: row.height, x: 0, y: row.top, toJSON: () => ({}) }) as DOMRect
  }
  return { container, actions, el }
}

describe(`fixtures/list-reorder.json dropSlot (Solid)`, () => {
  for (const c of (fixture as unknown as { dropSlot: Case[] }).dropSlot) {
    it(c.name, () => {
      const { actions, el } = mount(c.rows, c.maxDepth)
      const source = c.rows.find((r) => r.key === c.drag)
      if (!source) {
        expect(el(c.drag)).toBeNull()
        expect(c.move).toBeNull()
        return
      }
      const y = source.top + source.height / 2
      pointer(el(c.drag)!.querySelector(`[data-xui-c="Text"]`) ?? el(c.drag)!, `pointerdown`, 10, y)
      pointer(window, `pointermove`, 40, c.at)
      expect(el(c.drag)!.style.opacity).toBe(`0.4`)
      pointer(window, `pointerup`, 40, c.at)
      expect(el(c.drag)!.style.opacity).toBe(``)
      expect(document.querySelector(`[data-reorder-indicator]`)).toBeNull()
      expect(actions.filter((a) => a.name === `move`).map((a) => a.context)).toEqual(c.move ? [c.move] : [])
    })
  }
})

describe(`a reorderable List (Solid)`, () => {
  const rows: Row[] = [
    { key: `a`, top: 0, height: 30 },
    { key: `f`, top: 30, height: 30, group: true },
    { key: `f1`, top: 60, height: 30, depth: 1 },
  ]
  it(`items carry the DOM contract: key, depth, group, indent = the TreeGuides gutter`, () => {
    const { el } = mount(rows, 2)
    expect(el(`a`)!.getAttribute(`data-reorder-depth`)).toBe(`0`)
    expect(el(`a`)!.hasAttribute(`data-reorder-group`)).toBe(false)
    expect(el(`f`)!.getAttribute(`data-reorder-group`)).toBe(`true`)
    expect(el(`f1`)!.getAttribute(`data-reorder-depth`)).toBe(`1`)
    expect(el(`f1`)!.getAttribute(`data-reorder-indent`)).toBe(String(TREE_GUIDE_COLUMN))
  })
  it(`Esc cancels: no move, the row undimmed`, () => {
    const { actions, el } = mount(rows, 2)
    pointer(el(`a`)!, `pointerdown`, 10, 15)
    pointer(window, `pointermove`, 40, 75)
    expect(document.querySelector(`[data-reorder-indicator]`)).not.toBeNull()
    window.dispatchEvent(new KeyboardEvent(`keydown`, { key: `Escape`, bubbles: true, cancelable: true }))
    pointer(window, `pointerup`, 40, 75)
    expect(el(`a`)!.style.opacity).toBe(``)
    expect(actions.filter((a) => a.name === `move`)).toEqual([])
  })
  it(`a windowed List (past the threshold): its rendered rows drag`, () => {
    const many: Row[] = Array.from({ length: 60 }, (_, i) => ({ key: `r${i}`, top: i * 30, height: 30 }))
    const { container, actions, el } = mount(many, 1)
    expect(container.querySelector(`.xui-list-window > .xui-list-item[data-reorder-key="r0"]`)).not.toBeNull()
    pointer(el(`r0`)!, `pointerdown`, 10, 15)
    pointer(window, `pointermove`, 40, 70)
    pointer(window, `pointerup`, 40, 70)
    expect(actions.filter((a) => a.name === `move`).map((a) => a.context)).toEqual([{ id: `r0`, before: `r2` }])
  })
  it(`a press inside a field never starts a drag`, () => {
    const { container, actions, el } = mount(rows, 2)
    const input = document.createElement(`input`)
    el(`a`)!.appendChild(input)
    pointer(input, `pointerdown`, 10, 15)
    pointer(window, `pointermove`, 40, 75)
    pointer(window, `pointerup`, 40, 75)
    expect(container.ownerDocument.querySelector(`[data-reorder-indicator]`)).toBeNull()
    expect(actions.filter((a) => a.name === `move`)).toEqual([])
  })
  it(`a horizontal or non-reorderable List carries no contract`, () => {
    const { container } = mount(rows, 2, { direction: `horizontal` })
    expect(container.querySelector(`[data-reorder-key]`)).toBeNull()
    cleanup()
    const flat = mount(rows, 2, { reorderable: false })
    expect(flat.container.querySelector(`[data-reorder-key]`)).toBeNull()
  })
})
