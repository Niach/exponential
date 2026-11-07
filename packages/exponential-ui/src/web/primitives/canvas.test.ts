// Round 8: the web painter replays fixtures/canvas.json (`compile`): every
// packed frame parses to its end under the shared Canvas2D painter, and its
// `text` ops come out as ONE `fillText` each, in source order, after every
// shape, with the op's font, weight, size × scale, physical align, the
// baseline rule and the straight RGBA colour. A recording context stands in
// for Canvas2D (the pixels are Chromium's: conformance/motion-parity.ts).
import { afterAll, beforeAll, describe, expect, it } from "bun:test"
import fixture from "../../../fixtures/canvas.json"
import { builtinTheme } from "../../themes"
import { motionConcat, paintMotionFrame, type MotionContext, type MotionMatrix } from "./motion-paint"
import { canvasThemeFonts } from "./canvas"

type TextOp = { op: `text`; transform: number[]; text: string; font: `sans` | `mono`; weight: string; size: number; align: string; baseline: string; color: number[] }
type Compile = { id: string; packed: number[]; list: { ops: ({ op: string } | TextOp)[] } }
const cases = (fixture as unknown as { compile: Compile[] }).compile

const ASCENT = 0.9
const DESCENT = 0.25
type Call = { kind: string; args: unknown[]; state: Record<string, unknown> }

/** A Canvas2D stand-in that records draw calls with the state they ran under. */
function recorder(): { ctx: MotionContext; calls: Call[] } {
  const calls: Call[] = []
  const state: Record<string, unknown> = { font: ``, textAlign: `start`, textBaseline: `alphabetic`, direction: `inherit`, fillStyle: ``, transform: [1, 0, 0, 1, 0, 0] }
  const saved: Record<string, unknown>[] = []
  const fontPx = () => Number(/(\d+(\.\d+)?)px/.exec(String(state.font))?.[1] ?? 10)
  const target: Record<string, unknown> = {
    save: () => saved.push({ ...state }),
    restore: () => Object.assign(state, saved.pop()),
    setTransform: (...m: number[]) => (state.transform = m),
    clearRect: () => {},
    clip: () => {},
    drawImage: () => {},
    getImageData: () => ({ data: new Uint8ClampedArray(4) }),
    putImageData: () => {},
    fill: () => calls.push({ kind: `fill`, args: [], state: { ...state } }),
    stroke: () => calls.push({ kind: `stroke`, args: [], state: { ...state } }),
    setLineDash: () => {},
    createLinearGradient: () => ({ addColorStop: () => {} }),
    createRadialGradient: () => ({ addColorStop: () => {} }),
    measureText: () => ({ fontBoundingBoxAscent: fontPx() * ASCENT, fontBoundingBoxDescent: fontPx() * DESCENT }),
    fillText: (...args: unknown[]) => calls.push({ kind: `text`, args, state: { ...state } }),
  }
  const ctx = new Proxy(target, {
    get: (t, k) => (k in t ? t[k as string] : state[k as string]),
    set: (_t, k, v) => {
      state[k as string] = v
      return true
    },
  }) as unknown as MotionContext
  return { ctx, calls }
}

const saved = (globalThis as Record<string, unknown>).Path2D
beforeAll(() => {
  ;(globalThis as Record<string, unknown>).Path2D = class {
    moveTo() {}
    lineTo() {}
    bezierCurveTo() {}
    closePath() {}
  }
})
afterAll(() => {
  ;(globalThis as Record<string, unknown>).Path2D = saved
})

const WEIGHTS: Record<string, number> = { regular: 400, medium: 500, semibold: 600, bold: 700 }
const ALIGNS: Record<string, string> = { start: `left`, center: `center`, end: `right` }
const fonts = { sans: `TestSans, sans-serif`, mono: `TestMono, monospace` }

describe(`canvas.json › the web painter (packed VERSION 2, text ops)`, () => {
  for (const c of cases) {
    for (const base of [[1, 0, 0, 1, 0, 0], [2, 0, 0, 2, 10, 4]] as MotionMatrix[]) {
      it(`${c.id} under [${base}]`, () => {
        const { ctx, calls } = recorder()
        const pool = { take: () => ({ canvas: { width: 1, height: 1 }, ctx }), give: () => {}, nativeFilters: () => false }
        expect(paintMotionFrame(ctx, 200, 200, c.packed, base, [], pool as never, fonts)).toBe(true)
        const want = c.list.ops.filter((o): o is TextOp => o.op === `text`)
        const texts = calls.filter((k) => k.kind === `text`)
        expect(texts.map((t) => t.args[0])).toEqual(want.map((o) => o.text))
        // text draws above every shape
        const firstText = calls.findIndex((k) => k.kind === `text`)
        if (firstText >= 0) expect(calls.slice(firstText).every((k) => k.kind === `text`)).toBe(true)
        texts.forEach((t, k) => {
          const o = want[k]!
          const m = motionConcat(base, o.transform as unknown as MotionMatrix)
          const scale = Math.sqrt(Math.abs(m[0] * m[3] - m[1] * m[2]))
          const px = o.size * scale
          const tr = t.state.transform as number[]
          // the anchor = the op's local origin through base × transform
          expect(tr[4]).toBeCloseTo(m[4], 3)
          expect(tr[5]).toBeCloseTo(m[5], 3)
          // the transform's rotation only: the font is set at device px
          expect(tr[0]).toBeCloseTo(m[0] / scale, 4)
          expect(tr[1]).toBeCloseTo(m[1] / scale, 4)
          expect(t.state.font).toBe(`${WEIGHTS[o.weight]} ${px}px ${fonts[o.font]}`)
          expect(t.state.textAlign).toBe(ALIGNS[o.align])
          expect(t.state.direction).toBe(`ltr`)
          expect(t.state.textBaseline).toBe(`alphabetic`)
          const dy = o.baseline === `top` ? px * ASCENT : o.baseline === `middle` ? (px * ASCENT - px * DESCENT) / 2 : 0
          expect(t.args[1]).toBe(0)
          expect(t.args[2] as number).toBeCloseTo(dy, 4)
          const [r, g, b, a] = o.color
          expect(t.state.fillStyle).toBe(`rgb(${r! * 255} ${g! * 255} ${b! * 255} / ${a})`)
        })
      })
    }
  }
  it(`a version 1 stream is refused (nothing painted)`, () => {
    const { ctx, calls } = recorder()
    expect(paintMotionFrame(ctx, 10, 10, [1, 10, 10], [1, 0, 0, 1, 0, 0])).toBe(false)
    expect(calls).toEqual([])
  })
  it(`a truncated text op is refused`, () => {
    const { ctx } = recorder()
    expect(paintMotionFrame(ctx, 10, 10, [2, 10, 10, 7, 1, 0, 0, 1, 0, 0, 0, 400, 14, 0, 2, 0, 0, 0, 1, 5, 104], [1, 0, 0, 1, 0, 0])).toBe(false)
  })
})

describe(`canvasThemeFonts`, () => {
  it(`the theme's sans and mono stacks`, () => {
    const f = canvasThemeFonts(builtinTheme(`neutral`))
    expect(f.sans).toContain(builtinTheme(`neutral`).tokens.type.family.sans)
    expect(f.mono).toContain(builtinTheme(`neutral`).tokens.type.family.mono)
  })
})
