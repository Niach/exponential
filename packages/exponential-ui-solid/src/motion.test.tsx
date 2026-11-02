// EXP-1605 (React's src/motion.test.tsx): the Solid Motion's interactivity (README › Interactivity) over the
// real engine: clicks, Enter/Space, hovers and bound inputs reach the state
// machine of fixtures/motion-state.json; entered states, presses, hovers,
// visibility and completion reach the node's `on` as A2UI actions.

import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { fireEvent, render, waitFor } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested, type NestedNode } from "@exponential_app/eui"
import { initMotionWasm } from "@exponential_app/eui/motion"
import stateFixture from "@exponential_app/eui/fixtures/motion-state.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

const machineOf = (id: string) => (stateFixture.cases as { id: string; machine: Record<string, unknown> }[]).find((c) => c.id === id)!.machine
const ON = { press: { event: { name: `pressed` } }, hover: { event: { name: `hovered` } }, visible: { event: { name: `seen` } }, complete: { event: { name: `done` } }, state: { event: { name: `state` } } }

let frames: FrameRequestCallback[] = []
let now = 0
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
  g.requestAnimationFrame = (cb: FrameRequestCallback) => frames.push(cb)
  g.cancelAnimationFrame = () => {}
  g.Path2D = class {}
})
afterAll(() => Object.assign(globalThis, saved))

/** Advance the clock: run the queued frames `ms` later. */
const advance = (ms: number) => {
  now += ms
  const run = frames
  frames = []
  run.forEach((f) => f(now))
}

function mount(props: Record<string, unknown>, data: Record<string, unknown> = {}) {
  const actions: SurfaceActionEvent[] = []
  const root = reduceNested({ id: `m`, component: `Motion`, props: { lottie: stateFixture.lottie, label: `Like`, ...props }, on: ON } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).root
  const r = render(() => <ExponentialSurface root={root} data={data} theme="neutral" id="ms" host={{ onAction: (e) => void actions.push(e) }} />)
  const el = r.container.querySelector<HTMLElement>(`[data-xui-id="m"]`)!
  const names = () => actions.splice(0).map((a) => (a.name === `state` ? `state:${String(a.context.state)}` : a.name))
  return { el, names }
}

describe(`Motion interactivity (M4)`, () => {
  it(`on.press makes a focusable button named by the label`, () => {
    const { el } = mount({})
    expect(el.getAttribute(`role`)).toBe(`button`)
    expect(el.tabIndex).toBe(0)
    expect(el.getAttribute(`aria-label`)).toBe(`Like`)
  })

  it(`toggle-on-click: clicks and Enter post click; entered states → on.state`, async () => {
    const { el, names } = mount({ stateMachine: machineOf(`toggle-on-click`) })
    await waitFor(() => expect(el.dataset.state).toBe(`ready`))
    expect(names()).toEqual([`seen`, `state:off`])
    fireEvent.click(el)
    expect(names()).toEqual([`state:on`, `pressed`])
    fireEvent.keyDown(el, { key: `Enter` })
    expect(names()).toEqual([`state:off`, `pressed`])
    fireEvent.keyDown(el, { key: ` ` })
    expect(names()).toEqual([`state:on`, `pressed`])
    fireEvent.pointerEnter(el)
    expect(names()).toEqual([`hovered`])
  })

  it(`hover-in-out: pointer enter / leave; a finished state completes into idle`, async () => {
    const { el, names } = mount({ stateMachine: machineOf(`hover-in-out`) })
    await waitFor(() => expect(el.dataset.state).toBe(`ready`))
    names()
    fireEvent.pointerEnter(el)
    expect(names()).toEqual([`state:in`, `hovered`])
    fireEvent.pointerLeave(el)
    expect(names()).toEqual([`state:out`])
    advance(0)
    advance(2000)
    expect(names()).toEqual([`done`, `state:idle`])
  })

  it(`progress-thresholds: bound inputs pick the state; the final state completes once`, async () => {
    const actions: SurfaceActionEvent[] = []
    const set = (id: string, level: number) => ({ id, component: `Button`, props: { label: id }, on: { press: { functionCall: { call: `set`, args: { path: `/sm/level`, value: level } } } } })
    const root = reduceNested(
      { id: `s`, component: `Stack`, children: [{ id: `m`, component: `Motion`, props: { lottie: stateFixture.lottie, stateMachine: machineOf(`progress-thresholds`), inputs: { path: `/sm` } }, on: { ...ON, press: undefined } }, set(`b60`, 60), set(`b10`, 10), set(`b100`, 100), set(`b0`, 0)] } as unknown as NestedNode,
      { catalogId: CORE_CATALOG_ID }
    ).root
    const r = render(() => <ExponentialSurface root={root} data={{ sm: { level: 30 } }} theme="neutral" id="mp" host={{ onAction: (e) => void actions.push(e) }} />)
    const el = r.container.querySelector<HTMLElement>(`[data-xui-id="m"]`)!
    const names = () => actions.splice(0).map((a) => (a.name === `state` ? `state:${String(a.context.state)}` : a.name))
    const press = (id: string) => fireEvent.click(r.container.querySelector(`[data-xui-id="${id}"]`)!)
    await waitFor(() => expect(el.dataset.state).toBe(`ready`))
    // Without on.press: an image, not a button.
    expect(el.getAttribute(`role`)).not.toBe(`button`)
    expect(names()).toEqual([`seen`, `state:low`])
    press(`b60`)
    expect(names()).toEqual([`state:high`])
    press(`b10`)
    expect(names()).toEqual([`state:low`])
    press(`b100`)
    // The engine chains low → high → done in one write.
    expect(names()).toEqual([`state:high`, `state:done`, `done`])
    press(`b0`)
    advance(0)
    advance(5000)
    expect(names()).toEqual([])
  })
})
