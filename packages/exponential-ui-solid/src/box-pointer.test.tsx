// #EDE-247: interactions-round1.json `boxPointer` on Solid, like React's
// interactions-fixture.test.tsx.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface BoxPointerCase {
  name: string
  steps: ({ aux: string; button: `middle` | `right` } | { press: string } | { wheel: string; notches: number; axis?: `x` })[]
  events: { name: string; context: Record<string, unknown> }[]
}
const boxPointerFixture = (fixture as unknown as { boxPointer: { surface: NestedNode; cases: BoxPointerCase[] } }).boxPointer

describe(`fixtures/interactions-round1.json boxPointer`, () => {
  for (const c of boxPointerFixture.cases) {
    it(`boxPointer: ${c.name}`, () => {
      const actions: { name: string; context: Record<string, unknown> }[] = []
      const { container } = render(() => <ExponentialSurface root={reduceNested(boxPointerFixture.surface, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="bp" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />)
      const box = (id: string) => container.querySelector(`[data-xui-id="${id}"]`) as HTMLElement
      for (const s of c.steps) {
        if (`aux` in s) {
          const button = s.button === `middle` ? 1 : 2
          fireEvent.mouseDown(box(s.aux), { button })
          if (button === 2) fireEvent.contextMenu(box(s.aux), { button })
          fireEvent.mouseUp(box(s.aux), { button })
          fireEvent(box(s.aux), new MouseEvent(`auxclick`, { bubbles: true, cancelable: true, button }))
        } else if (`wheel` in s) {
          const delta = -3 * s.notches
          fireEvent.wheel(box(s.wheel), { deltaMode: 1, deltaX: s.axis === `x` ? delta : 0, deltaY: s.axis === `x` ? 0 : delta })
        } else fireEvent.click(box(s.press))
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
    })
  }
  it(`boxPointer: pixel deltas accumulate to a step per 100 px; the context menu and page scroll are kept only with the handlers`, () => {
    const actions: { name: string; context: Record<string, unknown> }[] = []
    const { container } = render(() => <ExponentialSurface root={reduceNested(boxPointerFixture.surface, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="bp" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />)
    const box = (id: string) => container.querySelector(`[data-xui-id="${id}"]`) as HTMLElement
    for (let i = 0; i < 5; i++) fireEvent.wheel(box(`a`), { deltaMode: 0, deltaY: 40 })
    expect(actions.map((a) => a.context.direction)).toEqual([`down`, `down`])
    expect(fireEvent.contextMenu(box(`a`))).toBe(false)
    expect(fireEvent.wheel(box(`a`), { deltaMode: 1, deltaY: 3 })).toBe(false)
    expect(fireEvent.contextMenu(box(`b`))).toBe(true)
    expect(fireEvent.wheel(box(`b`), { deltaMode: 1, deltaY: 3 })).toBe(true)
  })
})
