// Round 8 (docs/round-8-contract.md §1): the React Canvas over the shared
// `CanvasView` and the real engine: the box and a11y, a refused drawing as a
// paint error, and fixtures/canvas.json `pointer` through the DOM: press,
// move (once per frame, the last point) and leave reach the node's `on`
// only with `pointer: true`.

import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { act, fireEvent, render, waitFor } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested, type NestedNode } from "@exponential_app/eui"
import { initMotionWasm } from "@exponential_app/eui/motion"
import fixture from "@exponential_app/eui/fixtures/canvas.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

type PointerCase = { size: [number, number]; fit: string; box: [number, number]; points: { at: [number, number]; drawing: [number, number] }[] }
const ON = { press: { event: { name: `press` } }, move: { event: { name: `move` } }, leave: { event: { name: `leave` } } }
const shapes = [{ type: `rect`, position: [50, 25], size: [80, 30] }, { type: `fill`, color: `$color.primary` }, { type: `text`, text: `Hi`, position: [50, 25], align: `center`, baseline: `middle` }]

let frames: FrameRequestCallback[] = []
const saved: Record<string, unknown> = {}
beforeAll(() => {
  initMotionWasm(readFileSync(resolve(__dirname, `../../exponential-ui/wasm/motion/eui_motion_wasm_bg.wasm`)))
  const g = globalThis as Record<string, unknown>
  for (const k of [`requestAnimationFrame`, `cancelAnimationFrame`, `Path2D`]) saved[k] = g[k]
  g.requestAnimationFrame = (cb: FrameRequestCallback) => frames.push(cb)
  g.cancelAnimationFrame = () => {}
  g.Path2D = class {}
  HTMLCanvasElement.prototype.getContext = (() => null) as never
})
afterAll(() => Object.assign(globalThis, saved))

const flush = () =>
  act(() => {
    const run = frames
    frames = []
    run.forEach((f) => f(0))
  })

function mount(props: Record<string, unknown>, extra: Record<string, unknown> = {}) {
  const actions: SurfaceActionEvent[] = []
  const errors: string[] = []
  const root = reduceNested({ id: `c`, component: `Canvas`, props: { size: [100, 50], shapes, ...props }, on: ON, ...extra } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).root
  const r = render(<ExponentialSurface root={root} theme="neutral" id="cv" host={{ onAction: (e) => void actions.push(e), onPaintError: (e) => void errors.push(e.message) }} />)
  const el = r.container.querySelector<HTMLElement>(`[data-xui-id="c"]`)!
  const events = () => actions.splice(0).map((a) => (a.name === `leave` ? `leave` : `${a.name} ${a.context.x},${a.context.y}`))
  return { ...r, el, events, errors }
}

describe(`Canvas (round 8)`, () => {
  it(`role img named by label; decorative without one; the Motion box`, () => {
    const { el } = mount({ label: `Chart` })
    expect(el.getAttribute(`role`)).toBe(`img`)
    expect(el.getAttribute(`aria-label`)).toBe(`Chart`)
    expect(el.style.width).toBe(`100px`)
    expect(el.style.aspectRatio).toBe(`100 / 50`)
    const deco = mount({}, { id: `c` }).el
    expect(deco.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(deco.hasAttribute(`role`)).toBe(false)
  })

  it(`compiles over the engine; a refused drawing reports a paint error`, async () => {
    const ok = mount({ label: `Chart` })
    await waitFor(() => expect(ok.el.dataset.state).toBe(`ready`))
    const bad = mount({ shapes: [{ type: `merge` }] })
    await waitFor(() => expect(bad.el.dataset.state).toBe(`error`))
    expect(bad.errors[0]).toContain(`/shapes/0`)
  })

  for (const c of (fixture as unknown as { pointer: PointerCase[] }).pointer) {
    it(`pointer ${c.fit} ${c.size} in ${c.box}: press, move, leave in drawing units`, () => {
      const { el, events } = mount({ size: c.size, fit: c.fit, pointer: true })
      el.getBoundingClientRect = () => ({ left: 10, top: 20, width: c.box[0], height: c.box[1], right: 10 + c.box[0], bottom: 20 + c.box[1], x: 10, y: 20, toJSON: () => ({}) })
      for (const p of c.points) {
        fireEvent.click(el, { clientX: 10 + p.at[0], clientY: 20 + p.at[1] })
        const [got] = events()
        const [x, y] = got!.slice(6).split(`,`).map(Number)
        expect(got!.startsWith(`press `)).toBe(true)
        expect(Math.abs(x! - p.drawing[0])).toBeLessThanOrEqual(0.01)
        expect(Math.abs(y! - p.drawing[1])).toBeLessThanOrEqual(0.01)
      }
      const last = c.points.at(-1)!
      fireEvent.pointerMove(el, { clientX: 10, clientY: 20 })
      fireEvent.pointerMove(el, { clientX: 10 + last.at[0], clientY: 20 + last.at[1] })
      expect(events()).toEqual([])
      flush()
      const moved = events()
      expect(moved).toHaveLength(1)
      expect(moved[0]!.startsWith(`move `)).toBe(true)
      fireEvent.pointerLeave(el)
      expect(events()).toEqual([`leave`])
    })
  }

  it(`pointer false (the default): inert, no events`, () => {
    const { el, events } = mount({ label: `Chart` })
    el.getBoundingClientRect = () => ({ left: 0, top: 0, width: 100, height: 50, right: 100, bottom: 50, x: 0, y: 0, toJSON: () => ({}) })
    fireEvent.click(el, { clientX: 5, clientY: 5 })
    fireEvent.pointerMove(el, { clientX: 5, clientY: 5 })
    flush()
    fireEvent.pointerLeave(el)
    expect(events()).toEqual([])
    expect(el.hasAttribute(`tabindex`)).toBe(false)
  })
})
