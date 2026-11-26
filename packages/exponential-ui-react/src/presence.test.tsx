// Round 4 motion: fixtures/animations.json `presence` replayed through the
// React painter in jsdom: the container's painted children after every step
// (ghosts = `data-xui-exit`, inert, keeping their slot; the enter offset =
// the root's `animation-delay`), the exit's end from its timer (fake
// timers) instead of `finishExit`. The Chromium half is the conformance
// runner's (`__xuiPresence`).

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceSurface } from "@exponential_app/eui"
import { readPresence, setPointer, type PresenceScenario } from "@exponential_app/eui/web"
import type { FlatComponent } from "@exponential_app/eui"
import animations from "@exponential_app/eui/fixtures/animations.json"
import { ExponentialSurface } from "./surface"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const SCENARIOS = (animations as unknown as { presence: PresenceScenario[] }).presence

/** jsdom reads no sheet: the inline offset, else the node's own delay (0 under reduced motion). */
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
        let components = c.components
        let data: Record<string, unknown> = c.data
        const tree = () => {
          const r = reduceSurface(components, { catalogId: CORE_CATALOG_ID })
          return { root: r.root, templates: r.templates }
        }
        const view = render(<ExponentialSurface {...tree()} data={data} theme="neutral" id="p" />)
        c.steps.forEach((step, i) => {
          if (step.data) data = setPointer(data, step.data.path, step.data.value) as Record<string, unknown>
          if (step.components) components = step.components
          if (step.data || step.components) view.rerender(<ExponentialSurface {...tree()} data={data} theme="neutral" id="p" />)
          if (step.finishExit) act(() => void vi.advanceTimersByTime(1000))
          expect(painted(view.container, c, c.components.concat(step.components ?? [])), `step ${i}`).toEqual(step.expect)
        })
      } finally {
        window.matchMedia = matchMedia
      }
    })
  }
})
