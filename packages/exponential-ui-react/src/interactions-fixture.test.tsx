// Round 1 review: the reference half of fixtures/interactions-round1.json
// (NumberField stepping/rounding/text, Form check semantics, the unbound
// Table sort). The Rust core replays the same file in
// apps/desktop/crates/eui-core/tests/interactions_fixture.rs, so gpui
// and the natives (through the FFI) behave like this renderer.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import stripFixture from "@exponential_app/eui/fixtures/tabs-strip.json"
import { useEffect } from "react"
import { ExponentialSurface } from "./surface"
import { useSurface, type SurfaceState } from "./use-surface"
import { sortRows, type TableColumn } from "./natives/table"
import type { SurfaceActionEvent } from "./host"
import { keyRecorderCode } from "@exponential_app/eui/web"

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

interface SeverityCase {
  name: string
  data: Record<string, unknown>
  checks: unknown[]
  submitted: boolean
  errors: string[]
  shown: { severity: string; message: string } | null
  invalid: boolean
}

interface ButtonCheckCase {
  name: string
  data: Record<string, unknown>
  checks: unknown[]
  disabled: boolean
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
      const root = tree({ id: `n`, component: `NumberField`, props: { label: `N`, name: `n`, value: { "@path": `/v` }, ...c.props }, on: { change: { event: { name: `change` } } } })
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
          { id: `agree`, component: `Checkbox`, props: { label: `I agree`, name: `agree`, checked: { "@path": `/agreed` }, checks: c.checks } },
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

  // Round 9 §6.6: what a field does with each severity.
  for (const c of fixture.checkSeverities as SeverityCase[]) {
    it(`checkSeverities: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const root = tree({
        id: `f`,
        component: `Form`,
        props: { name: `f` },
        on: { submit: { event: { name: `submit` } }, invalid: { event: { name: `invalid` } } },
        children: [
          { id: `x`, component: `Input`, props: { label: `X`, name: `x`, value: `v`, validateOn: `submit`, checks: c.checks } },
          { id: `go`, component: `Button`, props: { label: `Send`, submit: true } },
        ],
      } as NestedNode)
      const { container } = render(<ExponentialSurface root={root} data={c.data} theme="neutral" id={`sv`} host={{ onAction: (e) => void actions.push(e) }} />)
      fireEvent.click(container.querySelector(`[data-xui-id="go"]`)!)
      const invalid = actions.find((a) => a.name === `invalid`)
      expect(((invalid?.context.errors ?? []) as { message: string }[]).map((e) => e.message)).toEqual(c.errors)
      expect(actions.some((a) => a.name === `submit`)).toBe(c.submitted)
      const part = container.querySelector(`[data-xui-id="x.error"]`)
      if (!c.shown) expect(part).toBeNull()
      else {
        expect(part!.textContent).toBe(c.shown.message)
        expect(part!.getAttribute(`role`)).toBe(c.shown.severity === `error` ? `alert` : `status`)
        expect(part!.getAttribute(`data-xs`)).toBe(c.shown.severity === `error` ? null : c.shown.severity)
      }
      expect(container.querySelector(`[data-xui-id="x.field"] input, input[data-xui-id="x.field"]`)!.hasAttribute(`aria-invalid`)).toBe(c.invalid)
    })
  }

  for (const c of fixture.buttonChecks as ButtonCheckCase[]) {
    it(`buttonChecks: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const root = tree({ id: `b`, component: `Button`, props: { label: `Go`, checks: c.checks }, on: { press: { event: { name: `press` } } } })
      const { container } = render(<ExponentialSurface root={root} data={c.data} theme="neutral" id={`bc`} host={{ onAction: (e) => void actions.push(e) }} />)
      const button = container.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement
      expect(button.disabled).toBe(c.disabled)
      fireEvent.click(button)
      expect(actions.some((a) => a.name === `press`)).toBe(!c.disabled)
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
      const root = tree({ id: `t`, component: `Table`, props: { columns: [{ key: `name`, label: `Name` }], rows: { "@path": `/rows` } }, on: { rowPress: { event: { name: `picked`, context: { value: { "@path": c.value } } } } } })
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
      const root = tree({ id: `c`, component: `ChoiceTiles`, props: { name: `c`, type: c.type, options, value: { "@path": `/v` } }, on: { change: { event: { name: `change` } } } })
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
          { id: `s`, component: `Select`, props: { label: `Status`, name: `s`, options, value: { "@path": `/v` } }, on: { change: { event: { name: `change` } } } },
          // /v as it is: `<unset>` when the path holds nothing.
          { id: `v`, component: `Text`, props: { text: { "@call": `fallback`, args: { value: { "@path": `/v` }, default: `<unset>` } } } },
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
  // A drag arms a one-shot click swallow that a 0 ms timer removes (the
  // release after a drag is no click): let it run, so the next describe's
  // first click (foldRow) is not eaten.
  afterEach(() => new Promise<void>((resolve) => setTimeout(resolve, 0)))
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

interface FoldCase {
  name: string
  open: boolean | null
  pressable: boolean
  rtl: boolean
  step: { press: `head` | `fold` } | { key: string; on: `head` | `fold` }
  events: string[]
  after: boolean
}

// #VAPP-284: a foldable Row (children): the head presses or folds, the fold
// button folds, ArrowRight / ArrowLeft open / close (swapped in rtl).
describe(`fixtures/interactions-round1.json foldRow`, () => {
  const fold = (fixture as unknown as { foldRow: { surface: NestedNode; cases: FoldCase[] } }).foldRow
  for (const c of fold.cases) {
    it(`foldRow: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const row: NestedNode = c.pressable ? fold.surface : { ...fold.surface, on: { change: fold.surface.on!.change! } }
      const root = tree(c.rtl ? { id: `rtl`, component: `Box`, style: { direction: `rtl` }, children: [row] } : row)
      const data = c.open === null ? {} : { open: c.open }
      const { container } = render(<ExponentialSurface root={root} data={data} theme="neutral" id={`fold`} host={{ onAction: (e) => void actions.push(e) }} />)
      const part = (name: `head` | `fold`) => container.querySelector(`[data-xui-id="issues.${name === `head` ? `head` : `head.fold`}"]`) as HTMLElement
      if (`press` in c.step) fireEvent.click(part(c.step.press))
      else fireEvent.keyDown(part(c.step.on), { key: c.step.key })
      expect(actions.map((a) => a.name)).toEqual(c.events)
      for (const a of actions.filter((x) => x.name === `fold`)) expect(a.context.open).toBe(c.after)
      expect(part(`head`).getAttribute(`aria-expanded`)).toBe(String(c.after))
    })
  }
})

interface TreeCase {
  name: string
  open: string[]
  rtl: boolean
  step: { press: string } | { key: string; on: string }
  events: string[]
  after: { open: string[]; picked: string | null; focus: string | null; visible?: string[] }
}

// #VAPP-285: the Tree: a leaf press selects, a branch press folds, a lazy
// branch fetches, the arrows follow treeKey over the visible rows.
describe(`fixtures/interactions-round1.json tree`, () => {
  const t = (fixture as unknown as { tree: { surface: NestedNode; rowIds: Record<string, string>; cases: TreeCase[] } }).tree
  const itemOf = (nodeId: string | null | undefined) => Object.entries(t.rowIds).find(([, id]) => id === nodeId)?.[0] ?? null
  for (const c of t.cases) {
    it(`tree: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const data: Record<string, unknown> = { open: c.open }
      const root = tree(c.rtl ? { id: `rtl`, component: `Box`, style: { direction: `rtl` }, children: [t.surface] } : t.surface)
      const { container } = render(
        <ExponentialSurface root={root} data={data} theme="neutral" id={`tree`} host={{ onAction: (e) => void actions.push(e) }} />
      )
      const row = (item: string) => container.querySelector(`[data-xui-id="${t.rowIds[item]}"]`) as HTMLElement
      if (`press` in c.step) fireEvent.click(row(c.step.press))
      else {
        row(c.step.on).focus()
        fireEvent.keyDown(row(c.step.on), { key: c.step.key })
      }
      expect(actions.map((a) => a.name)).toEqual(c.events)
      const focused = (document.activeElement as HTMLElement | null)?.getAttribute(`data-xui-id`)
      const start = `press` in c.step ? null : t.rowIds[c.step.on]
      expect(focused === start ? null : itemOf(focused)).toBe(c.after.focus)
      const open = [...container.querySelectorAll(`[role="treeitem"][aria-expanded="true"]`)].map((el) => itemOf(el.getAttribute(`data-xui-id`)))
      // /open as the DOM shows it: the open rows that are visible (a hidden row paints nothing).
      expect(open.sort()).toEqual(c.after.open.filter((item) => row(item)).sort())
      const selected = container.querySelector(`[role="treeitem"][aria-selected="true"]`)
      expect(itemOf(selected?.getAttribute(`data-xui-id`))).toBe(c.after.picked)
      if (c.after.visible) {
        const visible = [...container.querySelectorAll(`[data-xui-id^="files.row."]`)].filter((el) => /^files\.row\.\d+$/.test(el.getAttribute(`data-xui-id`) ?? ``)).map((el) => itemOf(el.getAttribute(`data-xui-id`)))
        expect(visible).toEqual(c.after.visible)
      }
    })
  }
})

interface FieldWriteCase {
  name: string
  seed: string
  pressPath?: string
  pressValue?: unknown
  steps: { op: `focus` | `type` | `hostWrite` | `press` | `blur`; text?: string; path?: string; value?: unknown }[]
  expect: { text: string; data?: unknown }
}

/** #VAPP-288: an Input bound to /draft and a Button whose press `set`s,
 *  on a `useSurface` state (so a host write is a real updateDataModel). */
function FieldWrites({ c, onState }: { c: FieldWriteCase; onState: (s: SurfaceState) => void }) {
  const surface = useSurface({
    surfaceId: `fw`,
    data: { draft: c.seed },
    initial: {
      id: `root`,
      component: `Stack`,
      children: [
        { id: `draft`, component: `Input`, props: { label: `Draft`, name: `draft`, value: { "@path": `/draft` } } },
        { id: `save`, component: `Button`, props: { label: `Save` }, ...(c.pressPath !== undefined ? { on: { press: { functionCall: { "@call": `set`, args: { path: c.pressPath, value: c.pressValue } } } } } : {}) },
      ],
    } as NestedNode,
  })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="fw" />
}

describe(`fixtures/interactions-round1.json fieldWrites`, () => {
  for (const c of (fixture as unknown as { fieldWrites: FieldWriteCase[] }).fieldWrites) {
    it(`fieldWrites: ${c.name}`, () => {
      let state!: SurfaceState
      const { container } = render(<FieldWrites c={c} onState={(s) => (state = s)} />)
      const input = () => container.querySelector(`[data-xui-id="draft"] input`) as HTMLInputElement
      for (const step of c.steps) {
        if (step.op === `focus`) fireEvent.focus(input())
        else if (step.op === `type`) fireEvent.change(input(), { target: { value: step.text } })
        else if (step.op === `hostWrite`) act(() => state.apply({ version: `1.0`, updateDataModel: { surfaceId: `fw`, path: step.path, value: step.value } }))
        else if (step.op === `press`) fireEvent.click(container.querySelector(`[data-xui-id="save"]`)!)
        else fireEvent.blur(input())
      }
      expect(input().value).toBe(c.expect.text)
      if (`data` in c.expect) expect((state.data as Record<string, unknown>).draft).toBe(c.expect.data)
    })
    // The same on a surface over local data (a `data` prop, no state).
    if (c.steps.every((s) => s.op !== `hostWrite`))
      it(`fieldWrites (local data): ${c.name}`, () => {
        const root = tree({
          id: `root`,
          component: `Stack`,
          children: [
            { id: `draft`, component: `Input`, props: { label: `Draft`, name: `draft`, value: { "@path": `/draft` } } },
            { id: `save`, component: `Button`, props: { label: `Save` }, ...(c.pressPath !== undefined ? { on: { press: { functionCall: { "@call": `set`, args: { path: c.pressPath, value: c.pressValue } } } } } : {}) },
          ],
        } as NestedNode)
        const { container } = render(<ExponentialSurface root={root} data={{ draft: c.seed }} theme="neutral" id="fwl" />)
        const input = () => container.querySelector(`[data-xui-id="draft"] input`) as HTMLInputElement
        for (const step of c.steps) {
          if (step.op === `focus`) fireEvent.focus(input())
          else if (step.op === `type`) fireEvent.change(input(), { target: { value: step.text } })
          else if (step.op === `press`) fireEvent.click(container.querySelector(`[data-xui-id="save"]`)!)
          else fireEvent.blur(input())
        }
        expect(input().value).toBe(c.expect.text)
      })
  }
})

// Round 8 (docs/round-8-contract.md §2): the checkbox cases. A press on a
// mixed box checks it and clears the dash (both paths written); a plain
// press toggles and leaves an unseeded /mixed unseeded; `change` carries
// {checked, indeterminate: false}.
interface CheckboxStep {
  change: { checked: boolean; indeterminate: boolean }
  checked: boolean
  mixed: boolean | null
  state: string
}
interface CheckboxCase {
  name: string
  data?: Record<string, unknown>
  unbound?: { checked: boolean; indeterminate: boolean }
  presses: number
  before: string
  steps: CheckboxStep[]
}
const checkboxFixture = (fixture as unknown as { checkbox: { surface: NestedNode; cases: CheckboxCase[] } }).checkbox

function CheckboxSurface({ c, onState, onAction }: { c: CheckboxCase; onState: (s: SurfaceState) => void; onAction: (e: SurfaceActionEvent) => void }) {
  const node = c.unbound ? { ...checkboxFixture.surface, props: { ...checkboxFixture.surface.props, ...c.unbound } } : checkboxFixture.surface
  const surface = useSurface({ surfaceId: `cb`, data: c.data ?? {}, initial: node as NestedNode })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="cb" host={{ onAction }} />
}

describe(`fixtures/interactions-round1.json checkbox`, () => {
  for (const c of checkboxFixture.cases) {
    it(`checkbox: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<CheckboxSurface c={c} onState={(s) => (state = s)} onAction={(e) => void actions.push(e)} />)
      const box = () => container.querySelector(`[role="checkbox"]`) as HTMLButtonElement
      const glyph = () => box().querySelector(`[data-slot]`)?.getAttribute(`data-slot`) ?? null
      expect(box().getAttribute(`aria-checked`)).toBe(c.before)
      expect(glyph()).toBe(c.before === `mixed` ? `Checkbox.check.indeterminate` : c.before === `true` ? `Checkbox.check` : null)
      for (let k = 0; k < c.presses; k++) {
        fireEvent.click(box())
        const step = c.steps[k]!
        const changes = actions.filter((a) => a.name === `change`)
        expect(changes.at(-1)!.context).toEqual(step.change)
        expect(box().getAttribute(`aria-checked`)).toBe(step.state)
        if (!c.unbound) {
          const data = state.data as Record<string, unknown>
          expect(data.checked).toBe(step.checked)
          expect(data.mixed ?? null).toBe(step.mixed)
        } else expect(box().getAttribute(`data-state`)).toBe(step.mixed ? `indeterminate` : step.checked ? `checked` : `unchecked`)
      }
    })
  }
})

interface OverlayAnchorCase {
  name: string
  step: { relayout?: boolean; scroll?: string; y?: number; insertAbove?: boolean; removeAnchor?: boolean }
  after: boolean
}

// #EXP-1739 overlayAnchor: an open anchored Popover closes once its anchor
// scrolls, moves or leaves the tree. jsdom lays nothing out, so a box's
// origin here is its place among its siblings (40px a row): a node inserted
// above the anchor moves it.
describe(`fixtures/interactions-round1.json overlayAnchor`, () => {
  const fx = (fixture as unknown as { overlayAnchor: { surface: NestedNode; above: NestedNode; cases: OverlayAnchorCase[] } }).overlayAnchor
  for (const c of fx.cases) {
    it(`overlayAnchor: ${c.name}`, async () => {
      const rect = vi.spyOn(HTMLElement.prototype, `getBoundingClientRect`).mockImplementation(function (this: HTMLElement) {
        let y = 0
        for (let el: Element | null = this.closest(`[data-xui-id]`); el?.previousElementSibling; el = el.previousElementSibling) y += 40
        return new DOMRect(0, y, 100, 40)
      })
      const shown = () => document.querySelector(`[data-xui-overlay="Popover"]`) !== null
      const frames = () => act(() => new Promise((resolve) => setTimeout(resolve, 50)))
      const { container, rerender } = render(<ExponentialSurface root={tree(fx.surface)} theme="neutral" id="oa" />)
      await frames()
      expect(shown(), `${c.name}: opens against its anchor`).toBe(true)
      const children = [...(fx.surface.children ?? [])] as NestedNode[]
      if (c.step.insertAbove) children.unshift(fx.above)
      const next = c.step.removeAnchor ? children.filter((child) => child.id !== `target`) : children
      rerender(<ExponentialSurface root={tree({ ...fx.surface, children: next })} theme="neutral" id="oa" />)
      if (c.step.scroll) fireEvent.scroll(container.querySelector(`[data-xui-id="${c.step.scroll}"]`)!)
      await frames()
      expect(shown(), c.name).toBe(c.after)
      rect.mockRestore()
    })
  }
})

interface BoxHoverCase {
  name: string
  steps: ({ enter: string } | { leave: string } | { press: string })[]
  events: { name: string; context: Record<string, unknown> }[]
}

// #EDE-65: a Box's `on.hover` fires as the pointer enters (Motion's rule).
// #FEED-201: `tilePress` = StatTile and Card presses, the same shape.
describe.each([`boxHover`, `tilePress`] as const)(`fixtures/interactions-round1.json %s`, (key) => {
  const hover = (fixture as unknown as Record<typeof key, { surface: NestedNode; cases: BoxHoverCase[] }>)[key]
  for (const c of hover.cases) {
    it(`${key}: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<ExponentialSurface root={tree(hover.surface)} theme="neutral" id="bh" host={{ onAction: (e) => void actions.push(e) }} />)
      const box = (id: string) => container.querySelector(`[data-xui-id="${id}"]`) as HTMLElement
      for (const s of c.steps) {
        if (`enter` in s) fireEvent.pointerEnter(box(s.enter))
        else if (`leave` in s) fireEvent.pointerLeave(box(s.leave))
        else fireEvent.click(box(s.press))
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
    })
  }
})

// #EXP-1629 keyRecorder: a press while idle starts recording (`start` +
// recording true), then every key and mouse buttons 3-5 go to `key`; blur
// stops it; a host write of recording false stops it silently.
type KeyRecorderMod = `ctrl` | `alt` | `shift` | `meta`
interface KeyRecorderStep {
  press?: boolean
  down?: string
  up?: string
  mods?: KeyRecorderMod[]
  modifiers?: KeyRecorderMod[]
  mouse?: number
  blur?: boolean
  hostWrite?: { path: string; value: unknown }
}
interface KeyRecorderCase {
  name: string
  steps: KeyRecorderStep[]
  events: { name: string; context: Record<string, unknown> }[]
  recording: boolean | null
}
const keyRecorderFixture = (fixture as unknown as { keyRecorder: { surface: NestedNode; cases: KeyRecorderCase[] } }).keyRecorder
const MOD_CODES: Record<KeyRecorderMod, string> = { ctrl: `ControlLeft`, alt: `AltLeft`, shift: `ShiftLeft`, meta: `MetaLeft` }
const DOM_BUTTON: Record<number, number> = { 1: 0, 2: 2, 3: 1, 4: 3, 5: 4 }
const modFlags = (mods: readonly KeyRecorderMod[]) => ({ ctrlKey: mods.includes(`ctrl`), altKey: mods.includes(`alt`), shiftKey: mods.includes(`shift`), metaKey: mods.includes(`meta`) })

function KeyRecorderSurface({ onState, onAction }: { onState: (s: SurfaceState) => void; onAction: (e: SurfaceActionEvent) => void }) {
  const surface = useSurface({ surfaceId: `kr`, data: {}, initial: keyRecorderFixture.surface })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="kr" host={{ onAction }} />
}

describe(`fixtures/interactions-round1.json keyRecorder`, () => {
  for (const c of keyRecorderFixture.cases) {
    it(`keyRecorder: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<KeyRecorderSurface onState={(s) => (state = s)} onAction={(e) => void actions.push(e)} />)
      const field = () => container.querySelector(`[data-xui-id="ptt.field"]`) as HTMLElement
      let held: KeyRecorderMod[] = []
      for (const s of c.steps) {
        if (s.press) fireEvent.click(field())
        else if (s.down !== undefined) fireEvent.keyDown(field(), { code: keyRecorderCode(s.down), key: s.down, ...modFlags(s.mods ?? []) })
        else if (s.up !== undefined) fireEvent.keyUp(field(), { code: keyRecorderCode(s.up), key: s.up, ...modFlags(s.mods ?? []) })
        else if (s.modifiers) {
          // One platform modifier-change event: a keydown of an added key, else a keyup of a released one.
          const added = s.modifiers.find((m) => !held.includes(m))
          const released = held.find((m) => !s.modifiers!.includes(m))
          const flags = modFlags(s.modifiers)
          if (added) fireEvent.keyDown(field(), { code: MOD_CODES[added], key: added, ...flags })
          else if (released) fireEvent.keyUp(field(), { code: MOD_CODES[released], key: released, ...flags })
          held = s.modifiers
        } else if (s.mouse !== undefined) fireEvent.mouseDown(field(), { button: DOM_BUTTON[s.mouse] })
        else if (s.blur) fireEvent.blur(field())
        else if (s.hostWrite) act(() => state.apply({ version: `1.0`, updateDataModel: { surfaceId: `kr`, path: s.hostWrite!.path, value: s.hostWrite!.value } }))
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).recording ?? null).toBe(c.recording)
      if (c.recording !== null) expect(field().getAttribute(`aria-pressed`)).toBe(String(c.recording))
      if (c.recording) expect(document.activeElement).toBe(field())
    })
  }
})

// #EXP-1760 pickerPick: IconPicker / ColorPicker are macros over a Popover;
// the trigger opens it, a pick (cell, swatch, No icon) writes `value`, fires
// `change` {value} and closes it (ClosePickerContext).
interface PickerCase {
  name: string
  surface: string
  data: Record<string, unknown>
  steps: { press: string }[]
  events: { name: string; context: Record<string, unknown> }[]
  value: unknown
  open: boolean
}
const pickerFixture = (fixture as unknown as { pickerPick: { surfaces: Record<string, NestedNode>; cases: PickerCase[] } }).pickerPick

function PickerSurface({ initial, data, onState, onAction }: { initial: NestedNode; data: Record<string, unknown>; onState: (s: SurfaceState) => void; onAction: (e: SurfaceActionEvent) => void }) {
  const surface = useSurface({ surfaceId: `pk`, data, initial })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="pk" host={{ onAction }} />
}

describe(`fixtures/interactions-round1.json pickerPick`, () => {
  for (const c of pickerFixture.cases) {
    it(`pickerPick: ${c.name}`, async () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      render(<PickerSurface initial={pickerFixture.surfaces[c.surface]!} data={c.data} onState={(s) => (state = s)} onAction={(e) => void actions.push(e)} />)
      for (const step of c.steps) {
        const target = document.querySelector(`[data-xui-id="${step.press}"]`) as HTMLElement | null
        expect(target, `${c.name}: ${step.press}`).not.toBeNull()
        await act(async () => void fireEvent.click(target!))
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).value ?? null).toEqual(c.value)
      expect(document.querySelector(`[data-xui-overlay="Popover"]`) !== null, `${c.name}: open`).toBe(c.open)
    })
  }
})

// #EDE-65 composerKeys: while menuKeys is on, Up, Down, Enter, Tab and
// Escape (no modifier, no IME composition) fire `key` {key} only; off,
// Enter submits as before.
interface ComposerKeyStep {
  type?: string
  key?: string
  mods?: KeyRecorderMod[]
  composing?: boolean
  handled?: boolean
  hostWrite?: { path: string; value: unknown }
}
interface ComposerKeyCase {
  name: string
  data: Record<string, unknown>
  steps: ComposerKeyStep[]
  events: { name: string; context: Record<string, unknown> }[]
  draft: string | null
}
const composerKeysFixture = (fixture as unknown as { composerKeys: { surface: NestedNode; cases: ComposerKeyCase[] } }).composerKeys

function ComposerKeysSurface({ data, onState, onAction }: { data: Record<string, unknown>; onState: (s: SurfaceState) => void; onAction: (e: SurfaceActionEvent) => void }) {
  const surface = useSurface({ surfaceId: `ck`, data: structuredClone(data), initial: composerKeysFixture.surface })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="ck" host={{ onAction }} />
}

describe(`fixtures/interactions-round1.json composerKeys`, () => {
  for (const c of composerKeysFixture.cases) {
    it(`composerKeys: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<ComposerKeysSurface data={c.data} onState={(s) => (state = s)} onAction={(e) => void actions.push(e)} />)
      const field = () => container.querySelector(`[data-xui-id="c"] textarea`) as HTMLTextAreaElement
      for (const s of c.steps) {
        if (s.type !== undefined) fireEvent.change(field(), { target: { value: s.type } })
        else if (s.key !== undefined) {
          const kept = fireEvent.keyDown(field(), { key: s.key, isComposing: s.composing ?? false, ...modFlags(s.mods ?? []) })
          expect(!kept, `${s.key} handled`).toBe(s.handled!)
        } else if (s.hostWrite) act(() => state.apply({ version: `1.0`, updateDataModel: { surfaceId: `ck`, path: s.hostWrite!.path, value: s.hostWrite!.value } }))
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).draft ?? null).toBe(c.draft)
    })
  }
})

// #EDE-65 R composerSlots: the slots sit in the Composer's controls row,
// in order leading · attach · trailing · send (DOM order = start → end; a
// CSS renderer mirrors it in RTL); a slot child is a live node.
interface ComposerSlotsCase {
  name: string
  props: Record<string, unknown>
  slots: Record<string, NestedNode>
  direction?: `rtl`
  stack?: string[]
  order: string[]
  data?: Record<string, unknown>
  field?: { rows: number; grown: boolean }
  hiddenAbove?: { path: string; value: unknown; stack: string[] }
  steps: { press: string }[]
  events: { name: string; context: Record<string, unknown> }[]
}
const composerSlotsFixture = (fixture as unknown as { composerSlots: { surface: NestedNode; cases: ComposerSlotsCase[] } }).composerSlots

/** The controls row's children by fixture name, start → end. */
function composerRowOrder(root: ParentNode): string[] {
  // #EXP-1762: a flat composer's row is its own children (the field between
  // the leading bar and the controls bar).
  const host = root.querySelector(`[data-xui-id="c"]`)!
  const flat = host.getAttribute(`data-r-layout`) === `flat`
  const row = flat ? Array.from(host.children).filter((el) => !el.classList.contains(`xui-composer-above`)).flatMap((el) => (el.classList.contains(`xui-composer-bar`) ? Array.from(el.children) : [el])) : Array.from(host.querySelector(`.xui-composer-bar`)!.children)
  return row.map((el) => {
    const c = el.className
    if (c.includes(`xui-Composer-field`)) return `field`
    if (c.includes(`xui-composer-leading`)) return `leading`
    if (c.includes(`xui-composer-trailing`)) return `trailing`
    if (c.includes(`xui-Composer-attachment`)) return `attachment`
    if (c.includes(`xui-Composer-send`)) return `send`
    return c
  })
}

function NestedSurface({ tree, host, data, onState }: { tree: NestedNode; host: Record<string, unknown>; data?: Record<string, unknown>; onState?: (s: SurfaceState) => void }) {
  const surface = useSurface({ surfaceId: `cs`, data: data ?? {}, initial: tree })
  useEffect(() => {
    onState?.(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="cs" host={host} />
}

/** #EXP-1762: the value the surface's stylesheets give `el` for `prop` (jsdom
 *  computes no cascade: the last matching rule's declaration). */
function cascaded(el: Element, prop: string): string {
  let value = ``
  const walk = (rules: CSSRuleList) => {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSStyleRule) {
        const v = rule.style.getPropertyValue(prop)
        if (v && el.matches(rule.selectorText)) value = v
      } else if (`cssRules` in rule) walk((rule as CSSGroupingRule).cssRules)
    }
  }
  for (const sheet of Array.from(document.styleSheets)) walk(sheet.cssRules)
  return el instanceof HTMLElement && el.style.getPropertyValue(prop) ? el.style.getPropertyValue(prop) : value
}

/** #EXP-1762: the composer's content top → bottom (`above`, `field`, `row`; a flat field is in the row). */
function composerStack(root: ParentNode): string[] {
  const host = root.querySelector(`[data-xui-id="c"]`)!
  const flat = host.getAttribute(`data-r-layout`) === `flat`
  const out: string[] = []
  for (const el of Array.from(host.children)) {
    const k = el.classList.contains(`xui-composer-above`) ? `above` : !flat && el.className.includes(`xui-Composer-field`) ? `field` : `row`
    if (out[out.length - 1] !== k) out.push(k)
  }
  return out
}

describe(`fixtures/interactions-round1.json composerSlots`, () => {
  for (const c of composerSlotsFixture.cases) {
    it(`composerSlots: ${c.name}`, () => {
      const actions: SurfaceActionEvent[] = []
      const composer = { ...composerSlotsFixture.surface, props: { ...composerSlotsFixture.surface.props, ...c.props }, ...(Object.keys(c.slots).length > 0 ? { slots: c.slots } : {}) } as NestedNode
      const root = { id: `root`, component: `Box`, style: { direction: c.direction ?? `ltr` }, children: [composer] } as NestedNode
      let state!: SurfaceState
      const { container } = render(<NestedSurface tree={root} data={c.data} onState={(s) => (state = s)} host={{ onAction: (e: SurfaceActionEvent) => void actions.push(e) }} />)
      expect(composerRowOrder(container)).toEqual(c.order)
      if (c.field) {
        // The field stretches across the row (grown by its min-height, never
        // a height) and its text starts at the recipe's padding-top
        // (neutral `$spacing.sm`).
        const field = container.querySelector(`[data-xui-id="c"] textarea`) as HTMLTextAreaElement
        expect(field.value.split(`\n`)).toHaveLength(c.field.rows)
        expect(cascaded(field, `align-self`)).toBe(`stretch`)
        expect(cascaded(field, `padding-top`)).toBe(`var(--xui-spacing-sm)`)
        expect(field.style.height).toBe(``)
      }
      if (c.stack) expect(composerStack(container)).toEqual(c.stack)
      for (const s of c.steps) fireEvent.click(container.querySelector(`[data-xui-id="${s.press}"]`)!)
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      // #EDE-65: a hidden `above` is the slot absent (no wrapper, so no gap);
      // shown it is the slot, hidden again it is gone.
      if (c.hiddenAbove) {
        const host = container.querySelector(`[data-xui-id="c"]`)!
        const { path, value, stack } = c.hiddenAbove
        expect(host.hasAttribute(`data-above`)).toBe(false)
        expect(host.querySelector(`.xui-composer-above`)).toBeNull()
        act(() => state.setData(`/${path}`, value))
        expect(composerStack(container)).toEqual(stack)
        expect(host.getAttribute(`data-above`)).toBe(`true`)
        expect(container.querySelector(`[data-xui-id="strip"]`)).not.toBeNull()
        act(() => state.setData(`/${path}`, false))
        expect(composerStack(container)).toEqual(c.stack)
        expect(host.hasAttribute(`data-above`)).toBe(false)
      }
    })
  }
})

// #EDE-65: the send/stop glyph's size is the `sendIcon` recipe part.
describe(`Composer sendIcon`, () => {
  for (const [theme, layout, size] of [[`neutral`, `flat`, `16px`], [`neutral`, `stacked`, `16px`], [`exponential`, `flat`, `24px`], [`exponential`, `stacked`, `16px`]] as const) {
    it(`${theme} ${layout}: the glyph is ${size}`, () => {
      function Host() {
        const surface = useSurface({ surfaceId: `si`, data: { draft: `hi` }, initial: { ...composerSlotsFixture.surface, props: { ...composerSlotsFixture.surface.props, layout } } as NestedNode })
        return <ExponentialSurface surface={surface} theme={theme} id="si" />
      }
      const { container } = render(<Host />)
      const glyph = container.querySelector(`[data-xui-part="Composer/send"] svg`)!
      expect(glyph.getAttribute(`data-xui-part`)).toBe(`Composer/sendIcon`)
      expect(cascaded(glyph, `width`)).toBe(size)
      expect(cascaded(glyph, `height`)).toBe(size)
    })
  }
})

// #EDE-65 R composerPaste: a paste with image files fires `attach` {files}
// and hands the bytes to onUpload; files without one do nothing; text alone
// is the field's own paste.
interface ComposerPasteCase {
  name: string
  data: Record<string, unknown>
  clipboard: { text?: string; files?: { name: string; size: number; type: string }[] }
  events: { name: string; context: Record<string, unknown> }[]
  uploads: string[]
  draft: string | null
}
const composerPasteFixture = (fixture as unknown as { composerPaste: { surface: NestedNode; cases: ComposerPasteCase[] } }).composerPaste

function ComposerPasteSurface({ data, onState, host }: { data: Record<string, unknown>; onState: (s: SurfaceState) => void; host: Record<string, unknown> }) {
  const surface = useSurface({ surfaceId: `cp`, data: structuredClone(data), initial: composerPasteFixture.surface })
  useEffect(() => {
    onState(surface)
  })
  return <ExponentialSurface surface={surface} theme="neutral" id="cp" host={host} />
}

describe(`fixtures/interactions-round1.json composerPaste`, () => {
  for (const c of composerPasteFixture.cases) {
    it(`composerPaste: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      const uploads: [string[], unknown][] = []
      const host = { onAction: (e: SurfaceActionEvent) => void actions.push(e), onUpload: (files: File[], t: unknown) => void uploads.push([files.map((f) => f.name), t]) }
      const { container } = render(<ComposerPasteSurface data={c.data} onState={(s) => (state = s)} host={host} />)
      const field = container.querySelector(`[data-xui-id="c"] textarea`) as HTMLTextAreaElement
      const files = (c.clipboard.files ?? []).map((f) => new File([`x`.repeat(f.size)], f.name, { type: f.type }))
      const kept = fireEvent.paste(field, { clipboardData: { files, getData: () => c.clipboard.text ?? `` } })
      // jsdom inserts nothing: a paste the Composer leaves is the browser's.
      if (kept && c.clipboard.text) fireEvent.change(field, { target: { value: `${field.value}${c.clipboard.text}` } })
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect(uploads.flatMap((u) => u[0])).toEqual(c.uploads)
      for (const u of uploads) expect((u[1] as { name: string }).name).toBe(`c`)
      expect((state.data as Record<string, unknown>).draft ?? null).toBe(c.draft)
    })
  }
})

// #EXP-1629 menuRows: an item's `identifier` sits between its icon and its
// label, its `detail` after the label (a submenu parent's too).
interface MenuRowCase {
  name: string
  items: unknown[]
  submenu: number | null
  rows: { key: string; parts: string[]; label: string; identifier?: string; detail?: string }[]
}
interface MenuPreviewStep {
  hover?: string | null
  overPreview?: boolean
  highlight?: string | null
  wait?: number
  press?: string
  shown: string | null
  texts?: string[]
  events: { name: string; context: Record<string, unknown> }[]
}
const menuRowsFixture = (fixture as unknown as { menuRows: MenuRowCase[] }).menuRows
const menuPreviewFixture = (fixture as unknown as { menuPreview: { menu: NestedNode; cases: { name: string; submenu: number | null; steps: MenuPreviewStep[] }[] } }).menuPreview

/** The part a row child paints (`null` = none of the fixture's). */
function menuRowPart(el: Element): string | null {
  const cls = el.getAttribute(`class`) ?? ``
  if (el.tagName.toLowerCase() === `svg`) return `icon`
  for (const [needle, part] of [[`xui-Menu-check`, `check`], [`xui-Menu-identifier`, `identifier`], [`xui-menu-label`, `label`], [`xui-Menu-detail`, `detail`], [`xui-Menu-shortcut`, `shortcut`], [`xui-Menu-submenuIndicator`, `submenuIndicator`]] as const) {
    if (cls.split(/\s+/).includes(needle)) return part
  }
  return null
}

/** Open menu `m` (and the top-level submenu `submenu`). */
function openMenu(container: HTMLElement, submenu: number | null) {
  fireEvent.pointerDown(container.querySelector(`.xui-Menu-trigger`)!, { button: 0, ctrlKey: false, pointerType: `mouse` })
  if (submenu !== null) {
    const row = menuRow(submenu.toString())!
    act(() => void fireEvent.keyDown(row, { key: `ArrowRight` }))
  }
}

/** The painted row at item path `key` (top level or the open submenu). */
function menuRow(key: string): HTMLElement | null {
  const path = key.split(`.`).map(Number)
  const content = document.querySelector(path.length === 1 ? `[data-xui-overlay="Menu"]` : `[data-xui-overlay="Menu.sub"]`)
  if (!content) return null
  const rows = Array.from(content.children).filter((c) => /^menuitem/.test(c.getAttribute(`role`) ?? ``))
  return (rows[path[path.length - 1]!] as HTMLElement | undefined) ?? null
}

describe(`fixtures/interactions-round1.json menuRows`, () => {
  for (const c of menuRowsFixture) {
    it(`menuRows: ${c.name}`, () => {
      const { container } = render(<ExponentialSurface root={tree({ id: `m`, component: `Menu`, props: { label: `More`, items: c.items } } as unknown as NestedNode)} theme="neutral" id="mr" />)
      openMenu(container, c.submenu)
      for (const r of c.rows) {
        const row = menuRow(r.key)
        expect(row, r.key).not.toBeNull()
        const parts = Array.from(row!.children).map(menuRowPart).filter((p): p is string => p !== null)
        expect({ key: r.key, parts }).toEqual({ key: r.key, parts: r.parts })
        expect(row!.querySelector(`.xui-menu-label`)!.textContent).toBe(r.label)
        expect(row!.querySelector(`.xui-Menu-identifier`)?.textContent ?? null).toBe(r.identifier ?? null)
        expect(row!.querySelector(`.xui-Menu-detail`)?.textContent ?? null).toBe(r.detail ?? null)
      }
    })
  }
})

// #EXP-1629 menuPreview: a row's preview shows beside it while hovered
// (after 150 ms) or highlighted (at once), hides 80 ms after both left; its
// inner events fire the Menu's `action`.
describe(`fixtures/interactions-round1.json menuPreview`, () => {
  afterEach(() => void vi.useRealTimers())
  for (const c of menuPreviewFixture.cases) {
    it(`menuPreview: ${c.name}`, () => {
      vi.useFakeTimers()
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<ExponentialSurface root={tree(menuPreviewFixture.menu)} theme="neutral" id="mpv" host={{ onAction: (e) => void actions.push(e) }} />)
      openMenu(container, c.submenu)
      let hovered: HTMLElement | null = null
      const content = (key: string) => document.querySelector(key.includes(`.`) ? `[data-xui-overlay="Menu.sub"]` : `[data-xui-overlay="Menu"]`) as HTMLElement
      for (const [i, s] of c.steps.entries()) {
        const before = actions.length
        if (`hover` in s) {
          if (hovered) act(() => void fireEvent.pointerLeave(hovered!, { pointerType: `mouse` }))
          hovered = s.hover ? menuRow(s.hover) : null
          if (hovered) {
            act(() => void fireEvent.pointerMove(hovered!, { pointerType: `mouse` }))
            act(() => void fireEvent.pointerEnter(hovered!, { pointerType: `mouse` }))
          }
        }
        if (`overPreview` in s) {
          const panel = document.querySelector(`[data-xui-menu-preview]`)!
          act(() => void (s.overPreview ? fireEvent.pointerEnter(panel, { pointerType: `mouse` }) : fireEvent.pointerLeave(panel, { pointerType: `mouse` })))
        }
        if (`highlight` in s) {
          if (s.highlight) {
            const row = menuRow(s.highlight)!
            act(() => void fireEvent.keyDown(content(s.highlight!), { key: `Shift` }))
            act(() => row.focus())
          } else {
            // No row highlighted: the focused row loses the focus.
            act(() => (document.activeElement as HTMLElement | null)?.blur())
          }
        }
        if (s.wait !== undefined) act(() => void vi.advanceTimersByTime(s.wait!))
        if (s.press) {
          const target = document.querySelector(`[data-xui-menu-preview] [data-xui-id="${s.press}"]`) as HTMLElement
          expect(target, s.press).not.toBeNull()
          act(() => void fireEvent.click(target))
        }
        const panel = document.querySelector(`[data-xui-menu-preview]`)
        expect({ step: i, shown: panel?.getAttribute(`data-xui-menu-preview`) ?? null }).toEqual({ step: i, shown: s.shown })
        if (panel) {
          expect(panel.getAttribute(`data-xui-id`)).toBe(`m.preview.${s.shown}`)
          for (const t of s.texts ?? []) expect(panel.textContent, t).toContain(t)
        }
        expect(actions.slice(before).filter((a) => a.name !== `picked`).map((a) => ({ name: a.name, context: a.context }))).toEqual(s.events)
        expect(document.querySelector(`[data-xui-overlay="Menu"]`), `step ${i}: the menu stays open`).not.toBeNull()
      }
    })
  }
})
