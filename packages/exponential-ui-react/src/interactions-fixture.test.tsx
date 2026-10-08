// Round 1 review: the reference half of fixtures/interactions-round1.json
// (NumberField stepping/rounding/text, Form check semantics, the unbound
// Table sort). The Rust core replays the same file in
// apps/desktop/crates/exponential-ui/tests/interactions_fixture.rs, so gpui
// and the natives (through the FFI) behave like this renderer.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import fixture from "@exponential-at/ui/fixtures/interactions-round1.json"
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
