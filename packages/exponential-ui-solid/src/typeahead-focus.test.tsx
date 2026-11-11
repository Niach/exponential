// #EDE-65: interactions-round1.json `typeaheadFocus` on Solid, like React's
// interactions-fixture.test.tsx: a Popover anchored on a text field (a
// typeahead) never takes the focus from it; every other Popover still
// focuses its content and returns the focus to its anchor on close.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface Step {
  do: `focusField` | `open` | `close` | `pressTrigger`
  open: boolean
  focus: `field` | `content` | `anchor`
}
interface Case {
  name: string
  surface: string
  data: Record<string, unknown>
  steps: Step[]
}
const fx = (fixture as unknown as { typeaheadFocus: { surfaces: Record<string, NestedNode>; cases: Case[] } }).typeaheadFocus
const settle = () => new Promise((resolve) => setTimeout(resolve, 20))

describe(`fixtures/interactions-round1.json typeaheadFocus`, () => {
  for (const c of fx.cases) {
    it(`typeaheadFocus: ${c.name}`, async () => {
      let state!: SurfaceState
      const { container } = render(() => {
        state = createSurface({ surfaceId: `tf`, data: structuredClone(c.data), initial: fx.surfaces[c.surface]! })
        return <ExponentialSurface surface={state} theme="neutral" id="tf" />
      })
      await settle()
      const field = () => container.querySelector<HTMLElement>(`[data-xui-id="c"] textarea, [data-xui-id="q"] input`)!
      const popover = () => document.querySelector(`[data-xui-overlay="Popover"]`)
      const where = (): string => {
        const at = document.activeElement
        if (!at || at === document.body) return `none`
        if (at === field()) return `field`
        if (popover()?.contains(at)) return `content`
        if (container.querySelector(`[data-xui-id="b"]`)?.contains(at)) return `anchor`
        return at.outerHTML.slice(0, 80)
      }
      for (const s of c.steps) {
        if (s.do === `focusField`) field().focus()
        else if (s.do === `open` || s.do === `close`) state.apply({ version: `1.0`, updateDataModel: { surfaceId: `tf`, path: `/open`, value: s.do === `open` } })
        else fireEvent.click(container.querySelector(`[data-xui-id="t"]`)!)
        await settle()
        expect(popover() !== null, `${c.name}: ${s.do} open`).toBe(s.open)
        expect(where(), `${c.name}: ${s.do} focus`).toBe(s.focus)
      }
    })
  }
})
