// #EDE-264 / #EDE-270: interactions-round1.json `eventArgs` on Solid, like
// React's interactions-fixture.test.tsx: an action's relative paths read the
// event's payload first (a Menu `select`'s {value} in a host function's
// args, a `set`'s value, the event's context), then the scope; a Toggle with
// only `on.change` presses and each press runs its function.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import type { HostPlugin } from "./host"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface Case {
  name: string
  surface: string
  data: Record<string, unknown>
  on: Record<string, unknown>
  steps: (`pick` | `press`)[]
  pressable?: boolean
  calls: { name: string; args: Record<string, unknown> }[]
  events: { name: string; context: Record<string, unknown> }[]
  after?: Record<string, unknown>
}
const fx = (fixture as unknown as { eventArgs: { surfaces: Record<string, NestedNode>; cases: Case[] } }).eventArgs
const settle = () => new Promise((resolve) => setTimeout(resolve, 20))

describe(`fixtures/interactions-round1.json eventArgs`, () => {
  for (const c of fx.cases) {
    it(`eventArgs: ${c.name}`, async () => {
      let state!: SurfaceState
      const calls: { name: string; args: Record<string, unknown> }[] = []
      const events: { name: string; context: unknown }[] = []
      const initial = structuredClone(fx.surfaces[c.surface]!) as NestedNode & { children: NestedNode[] }
      initial.children[0]!.on = c.on as NonNullable<NestedNode[`on`]>
      const host: HostPlugin = { onAction: (e) => void events.push({ name: e.name, context: e.context }), onFunctionCall: (call) => void calls.push({ name: call.name, args: call.args }) }
      const { container } = render(() => {
        state = createSurface({ surfaceId: `ea`, data: structuredClone(c.data), initial })
        return <ExponentialSurface surface={state} theme="neutral" id="ea" host={host} />
      })
      await settle()
      if (c.pressable) expect(container.querySelector(`[data-xui-id="t"]`)?.closest(`button`), `${c.name}: a button`).not.toBeNull()
      for (const step of c.steps) {
        if (step === `pick`) fireEvent.click(document.querySelector(`[data-xui-overlay="Menu"][data-state="open"]`)!.querySelector(`[role="menuitem"]`)!)
        else fireEvent.click(container.querySelector(`[data-xui-id="t"]`)!.closest(`button`)!)
        await settle()
      }
      expect(calls, `${c.name}: calls`).toEqual(c.calls)
      expect(events, `${c.name}: events`).toEqual(c.events)
      for (const [k, v] of Object.entries(c.after ?? {})) expect((state.data as Record<string, unknown>)[k], `${c.name}: /${k}`).toEqual(v)
    })
  }
})
