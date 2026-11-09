// #EDE-65: interactions-round1.json `composerKeys` on Solid, like React's
// interactions-fixture.test.tsx: while menuKeys is on, Up, Down, Enter, Tab
// and Escape (no modifier, no IME composition) fire `key` {key} only; off,
// Enter submits as before.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

type Mod = `ctrl` | `alt` | `shift` | `meta`
interface Step {
  type?: string
  key?: string
  mods?: Mod[]
  composing?: boolean
  handled?: boolean
  hostWrite?: { path: string; value: unknown }
}
interface Case {
  name: string
  data: Record<string, unknown>
  steps: Step[]
  events: { name: string; context: Record<string, unknown> }[]
  draft: string | null
}
const fx = (fixture as unknown as { composerKeys: { surface: NestedNode; cases: Case[] } }).composerKeys
const DOM_KEYS: Record<string, string> = { up: `ArrowUp`, down: `ArrowDown`, enter: `Enter`, tab: `Tab`, escape: `Escape` }
const flags = (mods: readonly Mod[]) => ({ ctrlKey: mods.includes(`ctrl`), altKey: mods.includes(`alt`), shiftKey: mods.includes(`shift`), metaKey: mods.includes(`meta`) })

describe(`fixtures/interactions-round1.json composerKeys`, () => {
  for (const c of fx.cases) {
    it(`composerKeys: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: { name: string; context: Record<string, unknown> }[] = []
      const { container } = render(() => {
        state = createSurface({ surfaceId: `ck`, data: structuredClone(c.data), initial: fx.surface })
        return <ExponentialSurface surface={state} theme="neutral" id="ck" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />
      })
      const field = () => container.querySelector(`[data-xui-id="c"] textarea`) as HTMLTextAreaElement
      for (const s of c.steps) {
        if (s.type !== undefined) {
          field().value = s.type
          fireEvent.input(field())
        } else if (s.key !== undefined) {
          const kept = fireEvent.keyDown(field(), { key: DOM_KEYS[s.key], isComposing: s.composing ?? false, ...flags(s.mods ?? []) })
          expect(!kept, `${s.key} handled`).toBe(s.handled!)
        } else if (s.hostWrite) state.apply({ version: `1.0`, updateDataModel: { surfaceId: `ck`, path: s.hostWrite.path, value: s.hostWrite.value } })
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect((state.data as Record<string, unknown>).draft ?? null).toBe(c.draft)
    })
  }
})
