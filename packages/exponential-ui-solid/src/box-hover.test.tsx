// #EDE-65: interactions-round1.json `boxHover` on Solid, like React's
// interactions-fixture.test.tsx: a Box's `on.hover` fires as the pointer
// enters (Motion's rule: no payload, leaving fires nothing).

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface BoxHoverCase {
  name: string
  steps: ({ enter: string } | { leave: string } | { press: string })[]
  events: { name: string; context: Record<string, unknown> }[]
}

// #FEED-201: `tilePress` = StatTile and Card presses, the same shape.
describe.each([`boxHover`, `tilePress`] as const)(`fixtures/interactions-round1.json %s`, (key) => {
  const hover = (fixture as unknown as Record<typeof key, { surface: NestedNode; cases: BoxHoverCase[] }>)[key]
  for (const c of hover.cases) {
    it(`${key}: ${c.name}`, () => {
      const actions: { name: string; context: Record<string, unknown> }[] = []
      const root = reduceNested(hover.surface, { catalogId: CORE_CATALOG_ID }).root
      const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="bh" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />)
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
