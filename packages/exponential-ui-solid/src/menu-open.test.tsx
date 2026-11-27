// #EDE-253: interactions-round1.json `menuOpen` on Solid, like React's
// interactions-fixture.test.tsx: a Menu's bindable `open` (Popover's rule):
// a host write opens a press or a context menu; a dismiss, a pick or the
// trigger writes the new value through and fires `change {open}`.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import type { SurfaceActionEvent } from "./host"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface Step {
  do: `open` | `close` | `escape` | `outside` | `pick` | `pressTrigger` | `none`
  shown: boolean
  data?: boolean
  changes: boolean[]
  selects?: string[]
}
interface Case {
  name: string
  surface: string
  data: Record<string, unknown>
  steps: Step[]
}
const fx = (fixture as unknown as { menuOpen: { surfaces: Record<string, NestedNode>; cases: Case[] } }).menuOpen
const settle = () => new Promise((resolve) => setTimeout(resolve, 20))

describe(`fixtures/interactions-round1.json menuOpen`, () => {
  for (const c of fx.cases) {
    it(`menuOpen: ${c.name}`, async () => {
      let state!: SurfaceState
      const actions: SurfaceActionEvent[] = []
      const { container } = render(() => {
        state = createSurface({ surfaceId: `mo`, data: structuredClone(c.data), initial: fx.surfaces[c.surface]! })
        return <ExponentialSurface surface={state} theme="neutral" id="mo" host={{ onAction: (e: SurfaceActionEvent) => void actions.push(e) }} />
      })
      await settle()
      const content = () => document.querySelector<HTMLElement>(`[data-xui-overlay="Menu"][data-state="open"]`)
      for (const [i, s] of c.steps.entries()) {
        const before = actions.length
        if (s.do === `open` || s.do === `close`) state.apply({ version: `1.0`, updateDataModel: { surfaceId: `mo`, path: `/open`, value: s.do === `open` } })
        else if (s.do === `escape`) fireEvent.keyDown(content()!, { key: `Escape` })
        else if (s.do === `outside`) fireEvent.pointerDown(document.body, { button: 0, pointerType: `mouse` })
        else if (s.do === `pick`) fireEvent.click(content()!.querySelector(`[role="menuitem"]`)!)
        else if (s.do === `pressTrigger`) fireEvent.pointerDown(container.querySelector(`[data-xui-id="t"]`)!.closest(`.xui-trigger`)!, { button: 0, ctrlKey: false, pointerType: `mouse` })
        await settle()
        const at = `${c.name}: step ${i} (${s.do})`
        expect(content() !== null, `${at} shown`).toBe(s.shown)
        if (s.data !== undefined) expect((state.data as Record<string, unknown>).open, `${at} data`).toBe(s.data)
        const fired = actions.slice(before)
        expect(fired.filter((a) => a.name === `change`).map((a) => (a.context as { open: boolean }).open), `${at} changes`).toEqual(s.changes)
        expect(fired.filter((a) => a.name === `select`).map((a) => (a.context as { value: string }).value), `${at} selects`).toEqual(s.selects ?? [])
        if (s.shown && c.surface === `context`) expect(content()!.getAttribute(`data-open-on`)).toBe(`contextmenu`)
      }
    })
  }
})
