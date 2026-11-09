// #EDE-65: bind-time.json's href/press extra cases on Solid: a MessageBubble
// (and a Row) is a button only while its href RESOLVES to a non-empty value
// or it has an `on.press`; a binding to null, "" or nothing leaves it plain.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import type { NestedNode, UiNode } from "@exponential_app/eui"
import bindTime from "@exponential_app/eui/fixtures/bind-time.json"
import { ExponentialSurface } from "./surface"
import { expectParity } from "./natives/table-chips-search.parity"

afterEach(() => cleanup())

interface HrefCase {
  name: string
  input: NestedNode
  expanded: UiNode
  datasets: { data: Record<string, unknown>; bound: UiNode }[]
}

const cases = (bindTime as unknown as { extra: HrefCase[] }).extra.filter((c) => /\/(href|press):/.test(c.name))

describe(`fixtures/bind-time.json href/press cases through Solid (+ DOM parity with React)`, () => {
  it(`carries the cases`, () => expect(cases.length).toBeGreaterThanOrEqual(5))
  for (const c of cases) {
    c.datasets.forEach((ds, i) => {
      it(`${c.name} dataset ${i}: pressable as the bound tree says`, () => {
        const { container } = render(() => <ExponentialSurface root={c.expanded} data={ds.data} theme="neutral" id={`bh${i}`} />)
        const root = container.querySelector(`[data-xui-id="${c.input.id}"]`) as HTMLElement
        const pressable = ds.bound.props.pressable === true
        expect(root.getAttribute(`data-pressable`)).toBe(pressable ? `true` : null)
        expect(root.getAttribute(`tabindex`)).toBe(pressable ? `0` : null)
        expect(root.getAttribute(`role`)).toBe(pressable ? `button` : ((ds.bound.accessibility as { role?: string } | undefined)?.role ?? null))
      })
      it(`${c.name} dataset ${i}: the same DOM as React`, async () => expectParity({ tree: c.input, data: ds.data }))
    })
  }
})
