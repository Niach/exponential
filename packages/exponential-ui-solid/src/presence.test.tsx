// Round 4 motion: fixtures/animations.json `presence` replayed through the
// Solid painter in jsdom (React's presence.test.tsx): ghosts =
// `data-xui-exit` keeping their slot, the enter offset = the root's
// `animation-delay`, each exit ended by its own timer (fake timers).

import { afterEach, describe, expect, it, vi } from "vitest"
import { createSignal } from "solid-js"
import { render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceSurface } from "@exponential_app/eui"
import { readPresence, setPointer, type PresenceScenario } from "@exponential_app/eui/web"
import type { FlatComponent } from "@exponential_app/eui"
import animations from "@exponential_app/eui/fixtures/animations.json"
import { ExponentialSurface } from "./surface"

afterEach(() => vi.useRealTimers())

const SCENARIOS = (animations as unknown as { presence: PresenceScenario[] }).presence

function painted(container: HTMLElement, c: PresenceScenario, components: FlatComponent[]) {
  return readPresence(container, c.container, c.first).map((p) => {
    const def = components.find((x) => x.id === p.id.split(`.`)[0]) as { style?: { animationDelay?: number } } | undefined
    return p.delayMs || p.exiting || c.reducedMotion ? p : { ...p, delayMs: def?.style?.animationDelay ?? 0 }
  })
}

describe(`animations.json presence (round 4 motion)`, () => {
  for (const c of SCENARIOS) {
    it(c.name, () => {
      vi.useFakeTimers()
      const matchMedia = window.matchMedia
      if (c.reducedMotion) window.matchMedia = ((q: string) => ({ matches: q.includes(`reduce`), media: q, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, onchange: null, dispatchEvent: () => false })) as typeof window.matchMedia
      try {
        const [state, setState] = createSignal({ components: c.components, data: c.data })
        const tree = () => reduceSurface(state().components, { catalogId: CORE_CATALOG_ID })
        const view = render(() => <ExponentialSurface root={tree().root} templates={tree().templates} data={state().data} theme="neutral" id="p" />)
        let components = c.components
        let data = c.data
        c.steps.forEach((step, i) => {
          if (step.data) data = setPointer(data, step.data.path, step.data.value) as Record<string, unknown>
          if (step.components) components = step.components
          if (step.data || step.components) setState({ components, data })
          // Each step a tick later: its exits start (and end) after the earlier ones.
          vi.advanceTimersByTime(1)
          for (let k = 0; step.finishExit && k < 10 && readPresence(view.container, c.container).some((p) => p.id === step.finishExit); k++) vi.advanceTimersToNextTimer()
          expect(painted(view.container, c, c.components.concat(step.components ?? [])), `step ${i}`).toEqual(step.expect)
        })
        view.unmount()
      } finally {
        window.matchMedia = matchMedia
      }
    })
  }
})
