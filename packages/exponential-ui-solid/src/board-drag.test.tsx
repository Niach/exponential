// genUI round 4: a Board in Solid (React's board-drag.test.tsx), replaying fixtures/board-drop.json
// `boardDropSlot` through the real DOM (jsdom has no layout: each column
// List's and card's box comes from the fixture). The column Lists carry
// `data-reorder-board`; a card drag fires the TARGET column's `move` with
// the macro's `column`.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/board-drop.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

interface Column {
  key: string
  left: number
  width: number
  rows: { key: string; top: number; height: number }[]
}
interface Case {
  name: string
  drag: string
  x: number
  y: number
  move: Record<string, string> | null
}

function pointer(target: EventTarget, type: string, x: number, y: number) {
  const e = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(e, `pointerId`, { value: 1 })
  Object.defineProperty(e, `pointerType`, { value: `mouse` })
  target.dispatchEvent(e)
}

const rect = (left: number, top: number, width: number, height: number) => () => ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top, toJSON: () => ({}) }) as DOMRect

function mount() {
  const columns = (fixture as unknown as { columns: Column[] }).columns
  const actions: SurfaceActionEvent[] = []
  const r = reduceNested({ id: `root`, component: `Box`, children: [{ id: `b`, component: `Board`, props: { columns: { "@path": `/cols` } }, on: { move: { event: { name: `move` } } } }] } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  const cols = columns.map((c) => ({ value: c.key, title: c.key, cards: c.rows.map((row) => ({ label: row.key, value: row.key })) }))
  const { container } = render(() => <ExponentialSurface root={r.root} templates={r.templates} data={{ cols }} theme="neutral" id="bd" host={{ onAction: (e) => void actions.push(e) }} />)
  const lists = Array.from(container.querySelectorAll<HTMLElement>(`[data-reorder-board]`))
  lists.forEach((l, i) => {
    const c = columns[i]
    l.getBoundingClientRect = rect(c.left, 0, c.width, 600)
    for (const row of c.rows) {
      const el = l.querySelector<HTMLElement>(`[data-reorder-key="${row.key}"]`)
      if (el) el.getBoundingClientRect = rect(c.left, row.top, c.width, row.height)
    }
  })
  const el = (key: string) => container.querySelector<HTMLElement>(`[data-reorder-key="${key}"]`)
  return { columns, lists, actions, el }
}

describe(`fixtures/board-drop.json boardDropSlot (Solid)`, () => {
  it(`the Board's column Lists share one board`, () => {
    const { lists, columns } = mount()
    expect(lists).toHaveLength(columns.length)
    expect(new Set(lists.map((l) => l.getAttribute(`data-reorder-board`)))).toEqual(new Set([`b`]))
  })
  for (const c of (fixture as unknown as { boardDropSlot: Case[] }).boardDropSlot) {
    it(c.name, () => {
      const { columns, actions, el } = mount()
      const column = columns.find((col) => col.rows.some((r) => r.key === c.drag))
      if (!column) {
        expect(el(c.drag)).toBeNull()
        expect(c.move).toBeNull()
        return
      }
      const row = column.rows.find((r) => r.key === c.drag)!
      pointer(el(c.drag)!, `pointerdown`, column.left + 10, row.top + row.height / 2)
      pointer(window, `pointermove`, c.x, c.y)
      expect(el(c.drag)!.style.opacity).toBe(`0.4`)
      const indicator = document.querySelector<HTMLElement>(`[data-reorder-indicator]`)
      expect(indicator?.style.display !== `none`).toBe(c.move !== null)
      pointer(window, `pointerup`, c.x, c.y)
      expect(el(c.drag)!.style.opacity).toBe(``)
      expect(actions.filter((a) => a.name === `move`).map((a) => a.context)).toEqual(c.move ? [c.move] : [])
    })
  }
})
