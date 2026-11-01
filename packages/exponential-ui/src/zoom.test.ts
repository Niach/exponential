// EXP-1586: fixtures/zoom.json replays against src/zoom.ts (the Rust core,
// SwiftUI and Compose replay the same file through the FFI).
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/zoom.json" with { type: "json" }
import * as zoom from "./zoom"

type Case = Record<string, any>
const frames = fixture.frames as Record<string, zoom.ZoomFrame>
const frameOf = (c: Case): zoom.ZoomFrame => (c.viewport ? { ...frames[c.frame], viewport: c.viewport } : frames[c.frame])
const fromOf = (c: Case, f: zoom.ZoomFrame): zoom.ZoomView => (c.from === `rest` ? zoom.zoomRestView(f) : c.from)

/** Numbers within 1e-6, everything else equal. */
function close(got: unknown, want: unknown, path = ``): void {
  if (typeof want === `number`) {
    expect(typeof got, path).toBe(`number`)
    expect(Math.abs((got as number) - want), `${path}: ${got} vs ${want}`).toBeLessThanOrEqual(1e-6)
  } else if (Array.isArray(want)) {
    expect(Array.isArray(got), path).toBe(true)
    expect((got as unknown[]).length, path).toBe(want.length)
    want.forEach((w, i) => close((got as unknown[])[i], w, `${path}[${i}]`))
  } else if (want !== null && typeof want === `object`) {
    expect(Object.keys(got as object).sort(), path).toEqual(Object.keys(want).sort())
    for (const k of Object.keys(want)) close((got as Record<string, unknown>)[k], (want as Record<string, unknown>)[k], `${path}.${k}`)
  } else expect(got, path).toEqual(want)
}

const runs: Record<string, (c: Case) => unknown> = {
  rest: (c) => {
    const f = frameOf(c)
    return { fit: zoom.zoomFitScale(f), limits: zoom.zoomLimits(f), restScale: zoom.zoomRestScale(f), view: zoom.zoomRestView(f), tall: zoom.zoomIsTall(f.natural) }
  },
  clamp: (c) => zoom.zoomClamp(c.view, frameOf(c)),
  zoomTo: (c) => zoom.zoomTo(fromOf(c, frameOf(c)), c.scale, c.anchor, frameOf(c)),
  wheel: (c) => zoom.zoomWheel(fromOf(c, frameOf(c)), c.deltaY, c.mode, c.anchor, frameOf(c)),
  scroll: (c) => zoom.zoomScroll(fromOf(c, frameOf(c)), c.deltaX, c.deltaY, frameOf(c)),
  ladder: (c) => zoom.zoomLadder(frameOf(c)),
  steps: (c) => zoom.zoomStepScale(c.scale, c.direction, frameOf(c)),
  doubleTap: (c) => zoom.zoomDoubleTap(fromOf(c, frameOf(c)), c.point, frameOf(c)),
  keys: (c) => zoom.zoomKey(fromOf(c, frameOf(c)), c.key, frameOf(c)),
  rubberBand: (c) => zoom.zoomRubberBand(c.overshoot, c.extent),
  drag: (c) => zoom.zoomDrag(fromOf(c, frameOf(c)), c.dx, c.dy, frameOf(c)),
  pinch: (c) => zoom.zoomPinch(fromOf(c, frameOf(c)), c.factor, c.anchor, frameOf(c)),
  momentum: (c) => zoom.zoomMomentum(fromOf(c, frameOf(c)), c.vx, c.vy, c.t, frameOf(c)),
  momentumActive: (c) => zoom.zoomMomentumActive(c.vx, c.vy, c.t),
  panRoom: (c) => zoom.zoomPanRoom(fromOf(c, frameOf(c)), frameOf(c)),
  resize: (c) => {
    const f = frameOf(c)
    const start = c.from === `rest` ? zoom.zoomRestView({ ...f, viewport: c.fromViewport }) : c.from
    return zoom.zoomResize(start, c.fromViewport, f)
  },
  menu: (c) => zoom.zoomMenu(fromOf(c, frameOf(c)), frameOf(c), `Fit`),
  bounds: (c) => zoom.zoomBounds(fromOf(c, frameOf(c)), frameOf(c)),
}

describe(`fixtures/zoom.json`, () => {
  for (const [group, run] of Object.entries(runs)) {
    const cases = (fixture as unknown as Record<string, Case[]>)[group]
    test(`${group} (${cases.length} cases)`, () => {
      expect(cases.length).toBeGreaterThan(0)
      for (const c of cases) close(run(c), c.expected, `${group} ${c.name}`)
    })
  }
  test(`percent`, () => {
    for (const [scale, label] of fixture.percent as [number, string][]) expect(zoom.zoomPercent(scale)).toBe(label)
  })
  test(`a zoom keeps the anchor's image point fixed`, () => {
    const f = frames.wide
    const from = zoom.zoomRestView(f)
    const anchor = { x: 400, y: 300 }
    const to = zoom.zoomTo(from, 1, anchor, f)
    const imageX = (anchor.x - from.x) / from.scale
    expect(Math.abs(to.x + imageX * to.scale - anchor.x)).toBeLessThan(1e-3)
  })
})
