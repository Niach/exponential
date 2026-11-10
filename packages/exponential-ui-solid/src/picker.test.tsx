// #EXP-1760: interactions-round1.json `pickerPick` on Solid, like React's
// interactions-fixture.test.tsx: the trigger opens the picker's Popover; a
// pick (a cell, a swatch, No icon) writes `value`, fires `change` {value}
// and closes it.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface Case {
  name: string
  surface: string
  data: Record<string, unknown>
  steps: { press: string }[]
  events: { name: string; context: Record<string, unknown> }[]
  value: unknown
  open: boolean
}
const fx = (fixture as unknown as { pickerPick: { surfaces: Record<string, NestedNode>; cases: Case[] } }).pickerPick
const frame = () => new Promise((resolve) => setTimeout(resolve, 0))

describe(`fixtures/interactions-round1.json pickerPick`, () => {
  for (const c of fx.cases) {
    it(`pickerPick: ${c.name}`, async () => {
      let state!: SurfaceState
      const actions: { name: string; context: Record<string, unknown> }[] = []
      render(() => {
        state = createSurface({ surfaceId: `pk`, data: c.data, initial: fx.surfaces[c.surface]! })
        return <ExponentialSurface surface={state} theme="neutral" id="pk" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />
      })
      for (const step of c.steps) {
        const target = document.querySelector(`[data-xui-id="${step.press}"]`) as HTMLElement | null
        expect(target, `${c.name}: ${step.press}`).not.toBeNull()
        fireEvent.click(target!)
        await frame()
      }
      await frame()
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).value ?? null).toEqual(c.value)
      const shown = document.querySelector(`[data-xui-overlay="Popover"][data-state="open"]`) !== null
      expect(shown, `${c.name}: open`).toBe(c.open)
    })
  }
})
