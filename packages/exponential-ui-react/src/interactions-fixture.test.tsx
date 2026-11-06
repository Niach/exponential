// Round 1 review: the reference half of fixtures/interactions-round1.json
// (NumberField stepping/rounding/text, Form check semantics, the unbound
// Table sort). The Rust core replays the same file in
// apps/desktop/crates/eui-core/tests/interactions_fixture.rs, so gpui
// and the natives (through the FFI) behave like this renderer.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import stripFixture from "@exponential_app/eui/fixtures/tabs-strip.json"
import { ExponentialSurface } from "./surface"
import { sortRows, type TableColumn } from "./natives/table"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

interface NumberCase {
  name: string
  props: Record<string, unknown>
  value: number | null
  op: `increment` | `decrement` | `commit` | `none`
  input?: string
  expect: { value: number; text: string }
}

interface CheckCase {
  name: string
  data: Record<string, unknown>
  checks: unknown[]
  failing: string[]
  submitted: boolean
}

interface SortCase {
  name: string
  column: TableColumn
  rows: Record<string, unknown>[]
  direction: `asc` | `desc`
  order: number[]
}

describe(`fixtures/interactions-round1.json`, () => {
  for (const c of fixture.numberField as NumberCase[]) {
    it(`NumberField: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const root = tree({ id: `n`, component: `NumberField`, props: { label: `N`, name: `n`, value: { path: `/v` }, ...c.props }, on: { change: { event: { name: `change` } } } })
      const data = c.value === null ? {} : { v: c.value }
      const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id={`nf`} host={{ onAction: (e) => void actions.push(e) }} />)
      const input = container.querySelector(`.xui-NumberField-input`) as HTMLInputElement
      if (c.op === `increment`) fireEvent.click(container.querySelector(`.xui-NumberField-increment`)!)
      if (c.op === `decrement`) fireEvent.click(container.querySelector(`.xui-NumberField-decrement`)!)
      if (c.op === `commit`) {
        fireEvent.focus(input)
        fireEvent.change(input, { target: { value: c.input } })
        fireEvent.blur(input)
      }
      const changes = actions.filter((a) => a.name === `change`)
      const value = changes.length ? changes.at(-1)!.context.value : c.value
      expect(value).toBe(c.expect.value)
      expect(input.value).toBe(c.expect.text)
    })
  }

  for (const c of fixture.checks as CheckCase[]) {
    it(`checks: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const root = tree({
        id: `f`,
        component: `Form`,
        props: { name: `f` },
        on: { submit: { event: { name: `submit` } }, invalid: { event: { name: `invalid` } } },
        children: [
          { id: `agree`, component: `Checkbox`, props: { label: `I agree`, name: `agree`, checked: { path: `/agreed` }, checks: c.checks } },
          { id: `go`, component: `Button`, props: { label: `Send`, submit: true } },
        ],
      } as NestedNode)
      const { container } = render(<ExponentialSurface root={root} data={c.data} theme="neutral" id={`ck`} host={{ onAction: (e) => void actions.push(e) }} />)
      fireEvent.click(container.querySelector(`[data-xui-id="go"]`)!)
      const invalid = actions.find((a) => a.name === `invalid`)
      const errors = ((invalid?.context.errors ?? []) as { message: string }[]).map((e) => e.message)
      expect(errors).toEqual(c.failing)
      expect(actions.some((a) => a.name === `submit`)).toBe(c.submitted)
    })
  }

  for (const c of fixture.tableSort as SortCase[]) {
    it(`Table sort: ${c.name}`, () => {
      expect(sortRows(c.rows, { key: c.column.key, direction: c.direction }, { label: c.column.key, ...c.column }, `en-US`)).toEqual(c.order)
    })
  }
})

// VAPP-206: a `rowPress` action's relative paths read the pressed row.
describe(`fixtures/interactions-round1.json rowPress`, () => {
  for (const c of (fixture as unknown as { rowPress: { name: string; rows: { id: string }[]; press: number; value: string; expect: string }[] }).rowPress) {
    it(`Table rowPress: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const root = tree({ id: `t`, component: `Table`, props: { columns: [{ key: `name`, label: `Name` }], rows: { path: `/rows` } }, on: { rowPress: { event: { name: `picked`, context: { value: { path: c.value } } } } } })
      const { container } = render(<ExponentialSurface root={root} data={{ rows: c.rows }} theme="neutral" id={`rp`} host={{ onAction: (e) => void actions.push(e) }} />)
      fireEvent.click(container.querySelector(`[role="row"][data-key="${c.rows[c.press]!.id}"]`)!)
      expect(actions.find((a) => a.name === `picked`)?.context.value).toBe(c.expect)
    })
  }
})

interface ChoiceCase {
  name: string
  type: `single` | `multiple`
  value: string | null
  disabled?: number
  presses: number[]
  expect: { value: string; changes: string[] }
}

describe(`fixtures/interactions-round1.json choiceTiles`, () => {
  for (const c of (fixture as unknown as { choiceTiles: ChoiceCase[] }).choiceTiles) {
    it(`ChoiceTiles: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const options = [`a`, `b`, `c`].map((v, i) => ({ label: v.toUpperCase(), value: v, image: `https://example.com/${v}.png`, ...(c.disabled === i ? { disabled: true } : {}) }))
      const root = tree({ id: `c`, component: `ChoiceTiles`, props: { name: `c`, type: c.type, options, value: { path: `/v` } }, on: { change: { event: { name: `change` } } } })
      const data = c.value === null ? {} : { v: c.value }
      const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id={`ct`} host={{ onAction: (e) => void actions.push(e) }} />)
      // A press on any part of tile i picks it (the label, the picture, the tile).
      const parts = [`tile`, `label`, `picture`]
      c.presses.forEach((i, k) => fireEvent.click(container.querySelector(`[data-xui-id="c.${parts[k % parts.length]}.${i}"]`)!))
      expect(actions.filter((a) => a.name === `change`).map((a) => a.context.value)).toEqual(c.expect.changes)
      const picked = [...container.querySelectorAll(`.xui-ChoiceTiles-tile[aria-checked="true"]`)].map((b) => b.getAttribute(`data-value`))
      expect(picked.join(`,`)).toBe(c.expect.value)
    })
  }
})

interface SelectCase {
  name: string
  value: string | null
  presses: number[]
  expect: { value: string | null; shows: string; changes: string[] }
}

// VAPP-146: an empty-string option is an ordinary value (Radix reserves `""`
// for "no value"; the painter maps it at the Radix boundary only).
describe(`fixtures/interactions-round1.json select`, () => {
  for (const c of (fixture as unknown as { select: SelectCase[] }).select) {
    it(`Select: ${c.name}`, () => {
      // A pick to or from "" must not flip Radix between controlled and
      // uncontrolled (Radix warns in development) or log any error.
      const errors = vi.spyOn(console, `error`).mockImplementation(() => {})
      const warnings = vi.spyOn(console, `warn`).mockImplementation(() => {})
      const actions: SurfaceActionEvent[] = []
      const options = [
        { label: `Any`, value: `` },
        { label: `Open`, value: `open` },
        { label: `Done`, value: `done` },
      ]
      const root = tree({
        id: `root`,
        component: `Box`,
        children: [
          { id: `s`, component: `Select`, props: { label: `Status`, name: `s`, options, value: { path: `/v` } }, on: { change: { event: { name: `change` } } } },
          // /v as it is: `<unset>` when the path holds nothing.
          { id: `v`, component: `Text`, props: { text: { call: `fallback`, args: { value: { path: `/v` }, default: `<unset>` } } } },
        ],
      })
      const data = c.value === null ? {} : { v: c.value }
      const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id={`sel`} host={{ onAction: (e) => void actions.push(e) }} />)
      const trigger = () => container.querySelector(`[data-xui-id="s.trigger"]`) as HTMLElement
      expect(container.querySelector(`[data-xui-error]`)).toBeNull()
      for (const i of c.presses) {
        fireEvent.keyDown(trigger(), { key: `Enter` })
        fireEvent.click(container.querySelectorAll(`[data-xui-overlay="Select"] [role="option"]`)[i]!)
      }
      expect(actions.filter((a) => a.name === `change`).map((a) => a.context.value)).toEqual(c.expect.changes)
      expect(trigger().textContent).toBe(c.expect.shows)
      expect(container.querySelector(`[data-xui-id="v"]`)?.textContent).toBe(c.expect.value === null ? `<unset>` : c.expect.value)
      expect([...errors.mock.calls, ...warnings.mock.calls].map((call) => String(call[0]))).toEqual([])
      errors.mockRestore()
      warnings.mockRestore()
    })
  }
})

interface HoldStep {
  do: string
  x?: number
  y?: number
  key?: string
  repeat?: boolean
  events: string[]
  pressed: boolean
  label: string
}

// VAPP-162 (round-5 contract §9): the Button hold, step by step through the DOM.
describe(`fixtures/interactions-round1.json buttonHold`, () => {
  for (const c of fixture.buttonHold as { name: string; props?: Record<string, unknown>; steps: HoldStep[] }[]) {
    it(c.name, () => {
      const actions: SurfaceActionEvent[] = []
      const base: Record<string, unknown> = { label: `Hold to talk`, holdingLabel: `Holding…`, hold: true }
      for (const [k, v] of Object.entries(c.props ?? {})) {
        if (v === null) delete base[k]
        else base[k] = v
      }
      const on = Object.fromEntries([`press`, `holdStart`, `holdEnd`, `holdCancel`].map((e) => [e, { event: { name: e } }]))
      const root = tree({ id: `b`, component: `Button`, props: base, on } as NestedNode)
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="bh" host={{ onAction: (e) => void actions.push(e) }} />)
      const button = container.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement
      button.getBoundingClientRect = () => ({ left: 0, top: 0, right: 120, bottom: 40, width: 120, height: 40, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect
      for (const s of c.steps) {
        const before = actions.length
        const at = { clientX: s.x ?? 0, clientY: s.y ?? 0, pointerId: 1, button: 0 }
        if (s.do === `pointerdown`) {
          fireEvent.pointerDown(button, at)
          fireEvent.mouseDown(button, at)
        } else if (s.do === `pointermove`) fireEvent.pointerMove(button, at)
        else if (s.do === `pointerup`) {
          fireEvent.pointerUp(button, at)
          fireEvent.mouseUp(button, at)
          // The browser's click follows a release over the box.
          if (s.x! >= 0 && s.x! <= 120 && s.y! >= 0 && s.y! <= 40) fireEvent.click(button, at)
        } else if (s.do === `pointercancel`) fireEvent.pointerCancel(button, { pointerId: 1 })
        else if (s.do === `keydown`) {
          button.focus()
          fireEvent.keyDown(button, { key: s.key, repeat: s.repeat === true })
        } else if (s.do === `keyup`) fireEvent.keyUp(button, { key: s.key })
        else if (s.do === `blur`) fireEvent.blur(button)
        expect(actions.slice(before).map((a) => a.event), `${s.do} events`).toEqual(s.events)
        const pressed = (button.getAttribute(`data-xs`) ?? ``).split(` `).includes(`pressed`)
        // `hold: false` paints its press through :active (CSS), not data-xs.
        if (base.hold === true) expect(pressed, `${s.do} pressed`).toBe(s.pressed)
        expect(button.textContent, `${s.do} label`).toBe(s.label)
      }
    })
  }
})

// VAPP-279: the strip Tabs reorder (drag + keyboard move), the core's
// tests/tabs_strip.rs twin. jsdom has no layout: each item's rect comes
// from the core-written fixtures/tabs-strip.json `layout` case at 900 px.
interface ReorderStep {
  do: `drag` | `key`
  phase?: `start` | `move` | `end`
  delta?: number
  key?: string
  order?: string[]
}
describe(`fixtures/interactions-round1.json tabsReorder`, () => {
  const labels = [`Inbox`, `Release notes`, `Calendar`, `Settings`, `Search`, `Drafts`]
  const icons: Record<number, string> = { 0: `message-circle`, 1: `pencil` }
  const frames = (stripFixture as unknown as { layout: { cases: Record<string, { expect: { frames: Record<string, number[]> } }> } }).layout.cases[`900`]!.expect.frames
  for (const c of (fixture as unknown as { tabsReorder: { name: string; tab: number; direction?: string; steps: ReorderStep[]; expect: unknown[] }[] }).tabsReorder) {
    it(c.name, () => {
      const rtl = c.direction === `rtl`
      const actions: SurfaceActionEvent[] = []
      const tabs = labels.map((label, i) => ({ label, value: `t${i}`, ...(icons[i] ? { icon: icons[i] } : {}) }))
      const root = tree({ id: `tabs`, component: `Tabs`, props: { variant: `strip`, tabs, value: `t4`, closable: `hover`, addable: true }, on: { reorder: { event: { name: `reorder` } } } } as NestedNode)
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="tr" direction={rtl ? `rtl` : `ltr`} host={{ onAction: (e) => void actions.push(e) }} />)
      const item = (i: number) => container.querySelector(`[data-xui-id="tabs.item.${i}"]`) as HTMLElement
      for (let i = 0; i < labels.length; i++) {
        const [x, y, w, h] = frames[`tabs.item.${i}`]!
        const left = rtl ? 900 - x! - w! : x!
        item(i).getBoundingClientRect = () => ({ left, top: y!, right: left + w!, bottom: y! + h!, width: w!, height: h!, x: left, y: y!, toJSON: () => ({}) }) as DOMRect
      }
      const list = container.querySelector(`[data-xui-id="tabs.list"]`) as HTMLElement
      list.getBoundingClientRect = () => ({ left: 0, top: 0, right: 900, bottom: 32, width: 900, height: 32, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect
      const trigger = container.querySelector(`[data-xui-id="tabs.tab.${c.tab}"]`) as HTMLElement
      const x = 100
      for (const s of c.steps) {
        if (s.do === `key`) {
          trigger.focus()
          fireEvent.keyDown(trigger, { key: s.key, shiftKey: true, ctrlKey: true })
        } else if (s.phase === `start`) {
          fireEvent.pointerDown(trigger, { pointerId: 1, button: 0, pointerType: `mouse`, clientX: x, clientY: 10 })
        } else {
          fireEvent.pointerMove(window, { pointerId: 1, pointerType: `mouse`, clientX: x + s.delta!, clientY: 10 })
          if (s.phase === `end`) fireEvent.pointerUp(window, { pointerId: 1, pointerType: `mouse`, clientX: x + s.delta!, clientY: 10 })
        }
        if (s.order) {
          const order = Array.from(container.querySelectorAll(`[data-xui-part="Tabs/item"]`)).map((el) => (el.getAttribute(`data-xui-id`) ?? ``).replace(`tabs.item.`, `t`))
          expect(order, `${c.name}: order after ${JSON.stringify(s)}`).toEqual(s.order)
        }
      }
      const fired = actions.filter((a) => a.name === `reorder`).map((a) => ({ from: a.context.from, to: a.context.to, value: a.context.value }))
      expect(fired).toEqual(c.expect)
    })
  }
})
