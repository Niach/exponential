// #EXP-1629: interactions-round1.json `keyRecorder` on Solid, like React's
// interactions-fixture.test.tsx: a press while idle starts recording, then
// every key and mouse buttons 3-5 go to `key`; blur stops it; a host write
// of recording false stops it silently.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { keyRecorderCode } from "@exponential_app/eui/web"
import { createSurface, type SurfaceState } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

type Mod = `ctrl` | `alt` | `shift` | `meta`
interface Step {
  press?: boolean
  down?: string
  up?: string
  mods?: Mod[]
  modifiers?: Mod[]
  mouse?: number
  blur?: boolean
  hostWrite?: { path: string; value: unknown }
}
interface Case {
  name: string
  steps: Step[]
  events: { name: string; context: Record<string, unknown> }[]
  recording: boolean | null
}
const fx = (fixture as unknown as { keyRecorder: { surface: NestedNode; cases: Case[] } }).keyRecorder
const MOD_CODES: Record<Mod, string> = { ctrl: `ControlLeft`, alt: `AltLeft`, shift: `ShiftLeft`, meta: `MetaLeft` }
const DOM_BUTTON: Record<number, number> = { 1: 0, 2: 2, 3: 1, 4: 3, 5: 4 }
const flags = (mods: readonly Mod[]) => ({ ctrlKey: mods.includes(`ctrl`), altKey: mods.includes(`alt`), shiftKey: mods.includes(`shift`), metaKey: mods.includes(`meta`) })

describe(`fixtures/interactions-round1.json keyRecorder`, () => {
  for (const c of fx.cases) {
    it(`keyRecorder: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: { name: string; context: Record<string, unknown> }[] = []
      const { container } = render(() => {
        state = createSurface({ surfaceId: `kr`, data: {}, initial: fx.surface })
        return <ExponentialSurface surface={state} theme="neutral" id="kr" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />
      })
      const field = () => container.querySelector(`[data-xui-id="ptt.field"]`) as HTMLElement
      let held: Mod[] = []
      for (const s of c.steps) {
        if (s.press) fireEvent.click(field())
        else if (s.down !== undefined) fireEvent.keyDown(field(), { code: keyRecorderCode(s.down), key: s.down, ...flags(s.mods ?? []) })
        else if (s.up !== undefined) fireEvent.keyUp(field(), { code: keyRecorderCode(s.up), key: s.up, ...flags(s.mods ?? []) })
        else if (s.modifiers) {
          // One platform modifier-change event: a keydown of an added key, else a keyup of a released one.
          const added = s.modifiers.find((m) => !held.includes(m))
          const released = held.find((m) => !s.modifiers!.includes(m))
          if (added) fireEvent.keyDown(field(), { code: MOD_CODES[added], key: added, ...flags(s.modifiers) })
          else if (released) fireEvent.keyUp(field(), { code: MOD_CODES[released], key: released, ...flags(s.modifiers) })
          held = s.modifiers
        } else if (s.mouse !== undefined) fireEvent.mouseDown(field(), { button: DOM_BUTTON[s.mouse] })
        else if (s.blur) fireEvent.blur(field())
        else if (s.hostWrite) state.apply({ version: `1.0`, updateDataModel: { surfaceId: `kr`, path: s.hostWrite.path, value: s.hostWrite.value } })
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).recording ?? null).toBe(c.recording)
      if (c.recording !== null) expect(field().getAttribute(`aria-pressed`)).toBe(String(c.recording))
      if (c.recording) expect(document.activeElement).toBe(field())
    })
  }
})
