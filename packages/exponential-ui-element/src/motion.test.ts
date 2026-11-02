// EXP-1605: <exponential-surface>'s Motion (the Solid native over the shared
// MotionView) reports its state machine's states and presses as xui-action
// events, like every renderer (README › Interactivity).

import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest"
import { initMotionWasm } from "@exponential_app/eui/motion"
import stateFixture from "@exponential_app/eui/fixtures/motion-state.json"
import type { NestedNode } from "@exponential_app/eui"
import { cleanup, mount, tick, tree } from "./test/helpers"

const saved: Record<string, unknown> = {}
beforeAll(() => {
  initMotionWasm(readFileSync(resolve(__dirname, `../../exponential-ui/wasm/motion/eui_motion_wasm_bg.wasm`)))
  const g = globalThis as Record<string, unknown>
  for (const k of [`IntersectionObserver`, `requestAnimationFrame`, `cancelAnimationFrame`, `Path2D`]) saved[k] = g[k]
  g.IntersectionObserver = class {
    constructor(private cb: IntersectionObserverCallback) {}
    observe(el: Element) {
      this.cb([{ isIntersecting: true, intersectionRatio: 1, target: el } as unknown as IntersectionObserverEntry], this as unknown as IntersectionObserver)
    }
    disconnect() {}
  }
  g.requestAnimationFrame = () => 1
  g.cancelAnimationFrame = () => {}
  g.Path2D = class {}
})
afterAll(() => Object.assign(globalThis, saved))
afterEach(cleanup)

describe(`Motion state machine (M4)`, () => {
  it(`a click toggles the machine; states and the press reach xui-action`, async () => {
    const machine = (stateFixture.cases as { id: string; machine: unknown }[]).find((c) => c.id === `toggle-on-click`)!.machine
    const seen: string[] = []
    const el = mount((e) => {
      e.addEventListener(`xui-action`, (ev) => {
        const d = (ev as CustomEvent<{ name: string; context: Record<string, unknown> }>).detail
        seen.push(d.name === `state` ? `state:${String(d.context.state)}` : d.name)
      })
      const t = tree({ id: `m`, component: `Motion`, props: { lottie: stateFixture.lottie, stateMachine: machine, label: `Like` }, on: { press: { event: { name: `liked` } }, state: { event: { name: `state` } } } } as unknown as NestedNode)
      e.root = t.root
    })
    const m = el.querySelector<HTMLElement>(`[data-xui-id="m"]`)!
    expect(m.getAttribute(`role`)).toBe(`button`)
    for (let k = 0; k < 50 && m.dataset.state !== `ready`; k++) await tick(10)
    expect(seen.splice(0)).toEqual([`state:off`])
    m.click()
    expect(seen.splice(0)).toEqual([`state:on`, `liked`])
  })
})
