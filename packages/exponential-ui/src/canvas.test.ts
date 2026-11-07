// Round 8: fixtures/canvas.json through the TS core (fit, pointer) and the
// committed WASM `CanvasPlayer` (compile, refusals, packed frames), as the
// web painters use them.
import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/canvas.json"
import { canvasA11y, canvasBoxStyle, canvasDeclaredSize, canvasFitMatrix, canvasToDrawing, type CanvasFit } from "./canvas"
import { checkboxAriaChecked, checkboxPress } from "./checkbox"
import { canvasPropsProblems, motionWasmSync } from "./motion/server"

type Compile = { id: string; canvas: unknown; slots?: unknown; slotIds: string[]; packed: number[] }
type Refuse = { id: string; canvas: unknown; issues: { code: string; path: string }[] }
const f = fixture as unknown as {
  compile: Compile[]
  refuse: Refuse[]
  fit: { size: [number, number]; box: [number, number]; fit: CanvasFit; matrix: number[] }[]
  pointer: { size: [number, number]; box: [number, number]; fit: CanvasFit; points: { at: [number, number]; drawing: [number, number] }[] }[]
}
const wasm = motionWasmSync()

describe(`canvas.json › compile (WASM CanvasPlayer)`, () => {
  for (const c of f.compile) {
    it(c.id, () => {
      const player = new wasm.CanvasPlayer(JSON.stringify(c.canvas))
      expect(JSON.parse(player.slotIds())).toEqual(c.slotIds)
      const got = Array.from(player.sample(c.slots ? JSON.stringify(c.slots) : ``))
      expect(got.length).toBe(c.packed.length)
      got.forEach((v, i) => expect(Math.abs(v - c.packed[i]!)).toBeLessThanOrEqual(1e-4 * Math.max(1, Math.abs(c.packed[i]!))))
      player.free()
    })
  }
})

describe(`canvas.json › refuse`, () => {
  for (const c of f.refuse) {
    it(c.id, () => {
      let issues: { code: string; path: string }[] = []
      try {
        new wasm.CanvasPlayer(JSON.stringify(c.canvas)).free()
      } catch (e) {
        issues = JSON.parse(String(e))
      }
      expect(issues.map((i) => [i.code, i.path])).toEqual(c.issues.map((i) => [i.code, i.path]))
    })
  }
  it(`canvasPropsProblems names them with their paths; a bound shapes waits for data`, () => {
    expect(canvasPropsProblems({ size: [10, 10], shapes: [] })).toEqual([])
    expect(canvasPropsProblems({ size: [10, 10], shapes: [{ type: `rect`, size: { keys: [] } }] })[0]).toContain(`/shapes/0/size`)
    expect(canvasPropsProblems({ size: [10, 10], shapes: { path: `/shapes` } })).toEqual([])
  })
})

describe(`canvas.json › fit and pointer (src/canvas.ts)`, () => {
  for (const c of f.fit) {
    it(`fit ${c.fit} ${c.size} in ${c.box}`, () => {
      canvasFitMatrix(c.size[0], c.size[1], c.box[0], c.box[1], c.fit).forEach((v, i) => expect(v).toBeCloseTo(c.matrix[i]!, 6))
    })
  }
  for (const c of f.pointer) {
    it(`pointer ${c.fit} in ${c.box}`, () => {
      for (const p of c.points) {
        const d = canvasToDrawing(p.at[0], p.at[1], c.size[0], c.size[1], c.box[0], c.box[1], c.fit)!
        expect(Math.abs(d.x - p.drawing[0])).toBeLessThanOrEqual(0.01)
        expect(Math.abs(d.y - p.drawing[1])).toBeLessThanOrEqual(0.01)
      }
    })
  }
})

describe(`canvas node helpers`, () => {
  it(`box: the drawing's px width shrinking to the parent, aspect kept; a style edge wins`, () => {
    expect(canvasDeclaredSize({ size: [160, 100] })).toEqual([160, 100])
    expect(canvasDeclaredSize({ size: [0, 100] })).toBeNull()
    expect(canvasBoxStyle([160, 100], undefined)).toEqual({ width: `160px`, maxWidth: `100%`, aspectRatio: `160 / 100` })
    expect(canvasBoxStyle([160, 100], { width: `100%` })).toEqual({ aspectRatio: `160 / 100` })
    expect(canvasBoxStyle([160, 100], { width: 10, height: 10 })).toEqual({})
  })
  it(`a11y: img named by label, hidden without one`, () => {
    expect(canvasA11y(`Visits`)).toEqual({ role: `img`, "aria-label": `Visits` })
    expect(canvasA11y(undefined)).toEqual({ "aria-hidden": `true` })
  })
  it(`checkbox: a press resolves mixed to checked, else toggles`, () => {
    expect(checkboxPress(false, true)).toEqual({ checked: true, indeterminate: false, writesIndeterminate: true, change: { checked: true, indeterminate: false } })
    expect(checkboxPress(true, true).checked).toBe(true)
    expect(checkboxPress(true, false)).toEqual({ checked: false, indeterminate: false, writesIndeterminate: false, change: { checked: false, indeterminate: false } })
    expect(checkboxAriaChecked(true, true)).toBe(`mixed`)
    expect(checkboxAriaChecked(true, undefined)).toBe(`true`)
  })
})

import interactions from "../fixtures/interactions-round1.json"

describe(`interactions-round1.json › checkbox (checkboxPress, the pure rule)`, () => {
  type Step = { change: unknown; checked: unknown; mixed: unknown; state: string }
  type C = { name: string; data?: Record<string, unknown>; unbound?: { checked: unknown; indeterminate: unknown }; presses: number; before: string; steps: Step[] }
  for (const c of (interactions as unknown as { checkbox: { cases: C[] } }).checkbox.cases) {
    it(c.name, () => {
      let checked: unknown = c.unbound ? c.unbound.checked : c.data?.checked
      let mixed: unknown = c.unbound ? c.unbound.indeterminate : c.data?.mixed
      expect(checkboxAriaChecked(checked, mixed)).toBe(c.before as `mixed`)
      for (const step of c.steps) {
        const r = checkboxPress(checked, mixed)
        checked = r.checked
        if (r.writesIndeterminate) mixed = false
        expect(r.change).toEqual(step.change as typeof r.change)
        expect(checked).toBe(step.checked)
        expect(mixed ?? null).toBe(step.mixed as boolean | null)
        expect(checkboxAriaChecked(checked, mixed)).toBe(step.state as `true`)
      }
    })
  }
})
