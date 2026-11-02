// EXP-1604: the web Motion helpers shared by React, Solid and the element
// (the Canvas2D painter itself is gated in Chromium: conformance/motion-parity.ts).
import { describe, expect, it } from "bun:test"
import { deflateRawSync } from "node:zlib"
import { builtinTheme } from "../../themes"
import stateFixture from "../../../fixtures/motion-state.json"
import { motionWasmSync } from "../../motion/server"
import { MotionStateDriver, motionA11y, motionBoxStyle, motionDeclaredSize, motionDotLottie, motionFit, motionFitMatrix, motionFromDotLottie, motionThemeSlots, type MotionEvent } from "./motion"

describe(`motion box (contract §4)`, () => {
  it(`unstyled: the composition's px width, shrinking to the width, aspect kept`, () =>
    expect(motionBoxStyle([200, 100], undefined)).toEqual({ width: `200px`, maxWidth: `100%`, aspectRatio: `200 / 100` }))
  it(`a style width or height wins; the aspect fills the other edge`, () => {
    expect(motionBoxStyle([200, 100], { width: 64 })).toEqual({ aspectRatio: `200 / 100` })
    expect(motionBoxStyle([200, 100], { height: 10 })).toEqual({ aspectRatio: `200 / 100` })
    expect(motionBoxStyle([200, 100], { width: 64, height: 10 })).toEqual({})
  })
  it(`the declared size: dialect size, inline Lottie w/h, none for src`, () => {
    expect(motionDeclaredSize({ layers: [], size: [48, 32] })).toEqual([48, 32])
    expect(motionDeclaredSize({ lottie: { w: 10, h: 20 } })).toEqual([10, 20])
    expect(motionDeclaredSize({ src: `/api/attachments/a` })).toBeNull()
  })
})

describe(`fit`, () => {
  it(`contain centres, cover overflows, fill stretches`, () => {
    expect(motionFitMatrix(100, 50, 160, 160, `contain`)).toEqual([1.6, 0, 0, 1.6, 0, 40])
    expect(motionFitMatrix(100, 50, 160, 160, `cover`)).toEqual([3.2, 0, 0, 3.2, -80, 0])
    expect(motionFitMatrix(100, 50, 160, 160, `fill`)).toEqual([1.6, 0, 0, 3.2, 0, 0])
    expect(motionFit(`nope`)).toBe(`contain`)
  })
})

describe(`theme slots (contract §2)`, () => {
  const theme = builtinTheme(`exponential`)
  it(`every $color slot takes the mode's colour; dark adds $mode; the node's slots last`, () => {
    const light = motionThemeSlots([`$color.primary`, `other`], theme, `light`, undefined)
    expect(Object.keys(light)).toEqual([`$color.primary`])
    const dark = motionThemeSlots([`$color.primary`], theme, `dark`, { "$color.primary": { p: { a: 0, k: [1, 0, 0, 1] } } })
    expect(dark.$mode).toBe(`dark`)
    expect(dark[`$color.primary`]).toEqual({ p: { a: 0, k: [1, 0, 0, 1] } })
    const k = (light[`$color.primary`] as { p: { k: number[] } }).p.k
    expect(k).toHaveLength(4)
    k.forEach((v) => expect(v >= 0 && v <= 1).toBe(true))
  })
})

describe(`a11y (contract §6)`, () => {
  it(`img named by the label; hidden without one`, () => {
    expect(motionA11y(`Loading`)).toEqual({ role: `img`, "aria-label": `Loading` })
    expect(motionA11y(``)).toEqual({ "aria-hidden": `true` })
  })
  it(`with on.press: a focusable button named by the label (M4 §4)`, () => {
    expect(motionA11y(`Like`, true)).toEqual({ role: `button`, tabIndex: 0, "aria-label": `Like` })
    expect(motionA11y(undefined, true)).toEqual({ role: `button`, tabIndex: 0 })
  })
})

// EXP-1605: the driver replays fixtures/motion-state.json over the real
// engine: every step's `entered` reaches `on.state`, a state change restarts
// the clock, a final state fires `on.complete` once.
describe(`state machines (fixtures/motion-state.json, M4 §1–4)`, () => {
  const wasm = motionWasmSync()
  type Snap = { state: string; entered: string[]; final: boolean; range: [number, number] }
  type Step = Snap & { step: { post?: string; layer?: string; set?: Record<string, unknown>; complete?: boolean } }
  type C = { id: string; machine: unknown; initial: Snap; steps: Step[] }
  const rig = () => {
    const player = new wasm.MotionPlayer(JSON.stringify(stateFixture.lottie), ``)
    const events: [MotionEvent, Record<string, unknown> | undefined][] = []
    let restarts = 0
    const driver = new MotionStateDriver(player, (e, p) => events.push([e, p]), () => restarts++)
    return { player, events, driver, restarts: () => restarts }
  }
  for (const c of (stateFixture as unknown as { cases: C[] }).cases) {
    it(c.id, () => {
      const { player, events, driver, restarts } = rig()
      const expectStep = (want: Snap, wasFinal: boolean) => {
        const wantEvents: [MotionEvent, Record<string, unknown> | undefined][] = want.entered.map((state) => [`state`, { state }])
        if (want.final && !wasFinal) wantEvents.push([`complete`, undefined])
        expect(events.splice(0)).toEqual(wantEvents)
        // The state's playback reached the clock.
        expect(player.posterFrame()).toBeCloseTo(want.range[0], 6)
      }
      expect(driver.setMachine(c.machine)).toBe(true)
      expect(restarts()).toBe(1)
      expectStep(c.initial, false)
      let final = c.initial.final
      for (const s of c.steps) {
        const before = restarts()
        if (s.step.post) driver.post(s.step.post, s.step.layer ?? ``)
        if (s.step.set) driver.setInputs(s.step.set)
        if (s.step.complete) {
          driver.done()
          // The finished play itself fires `complete` (then the machine moves on).
          expect(events.shift()).toEqual([`complete`, undefined])
        }
        expect(restarts() - before).toBe(s.entered.length > 0 ? 1 : 0)
        expectStep(s, final)
        final = s.final
      }
      player.free()
    })
  }
  it(`the same inputs again post nothing; a new machine restarts`, () => {
    const { player, events, driver, restarts } = rig()
    const c = (stateFixture.cases as { id: string; machine: unknown }[]).find((k) => k.id === `progress-thresholds`)!
    driver.setMachine(c.machine)
    driver.setInputs({ level: 60 })
    events.length = 0
    const n = restarts()
    driver.setInputs({ level: 60 })
    expect(restarts()).toBe(n)
    expect(driver.setMachine(c.machine)).toBe(true)
    expect(restarts()).toBe(n)
    expect(driver.setMachine(null)).toBe(false)
    expect(events).toEqual([])
    player.free()
  })
  it(`without a machine a finished play fires complete once per play`, () => {
    const { player, events, driver } = rig()
    driver.done()
    driver.done()
    driver.replay()
    driver.done()
    expect(events).toEqual([
      [`complete`, undefined],
      [`complete`, undefined],
    ])
    player.free()
  })
})

/** A minimal zip (deflate entries) for the dotLottie reader. */
function zip(files: Record<string, Uint8Array>): Uint8Array {
  const enc = new TextEncoder()
  const local: Uint8Array[] = []
  const central: Uint8Array[] = []
  let offset = 0
  for (const [name, data] of Object.entries(files)) {
    const n = enc.encode(name)
    const body = new Uint8Array(deflateRawSync(data))
    const h = new DataView(new ArrayBuffer(30))
    h.setUint32(0, 0x04034b50, true)
    h.setUint16(8, 8, true)
    h.setUint32(18, body.length, true)
    h.setUint32(22, data.length, true)
    h.setUint16(26, n.length, true)
    local.push(new Uint8Array(h.buffer), n, body)
    const c = new DataView(new ArrayBuffer(46))
    c.setUint32(0, 0x02014b50, true)
    c.setUint16(10, 8, true)
    c.setUint32(20, body.length, true)
    c.setUint32(24, data.length, true)
    c.setUint16(28, n.length, true)
    c.setUint32(42, offset, true)
    central.push(new Uint8Array(c.buffer), n)
    offset += 30 + n.length + body.length
  }
  const cdSize = central.reduce((a, b) => a + b.length, 0)
  const end = new DataView(new ArrayBuffer(22))
  end.setUint32(0, 0x06054b50, true)
  end.setUint16(8, Object.keys(files).length, true)
  end.setUint16(10, Object.keys(files).length, true)
  end.setUint32(12, cdSize, true)
  end.setUint32(16, offset, true)
  const parts = [...local, ...central, new Uint8Array(end.buffer)]
  const out = new Uint8Array(parts.reduce((a, b) => a + b.length, 0))
  let p = 0
  for (const part of parts) {
    out.set(part, p)
    p += part.length
  }
  return out
}

describe(`dotLottie`, () => {
  it(`reads the manifest's first animation and inlines its images`, async () => {
    const enc = new TextEncoder()
    const anim = { v: `5.7.0`, w: 10, h: 10, assets: [{ id: `i`, u: `/images/`, p: `a.png`, w: 1, h: 1 }], layers: [] }
    const bytes = zip({ "manifest.json": enc.encode(JSON.stringify({ animations: [{ id: `wave` }] })), "animations/wave.json": enc.encode(JSON.stringify(anim)), "images/a.png": new Uint8Array([1, 2, 3]) })
    const lottie = JSON.parse(await motionFromDotLottie(bytes)) as { assets: { p: string; e: number }[] }
    expect(lottie.assets[0]!.p).toBe(`data:image/png;base64,AQID`)
    expect(lottie.assets[0]!.e).toBe(1)
  })
  it(`reads the manifest's first state machine (s/, M4 §1)`, async () => {
    const enc = new TextEncoder()
    const anim = { v: `5.7.0`, w: 10, h: 10, layers: [] }
    const machine = { initial: `a`, states: [{ name: `a`, type: `PlaybackState` }] }
    const bytes = zip({
      "manifest.json": enc.encode(JSON.stringify({ animations: [{ id: `wave` }], stateMachines: [{ id: `main` }] })),
      "a/wave.json": enc.encode(JSON.stringify(anim)),
      "s/other.json": enc.encode(`{}`),
      "s/main.json": enc.encode(JSON.stringify(machine)),
    })
    expect((await motionDotLottie(bytes)).stateMachine).toEqual(machine)
    const plain = zip({ "manifest.json": enc.encode(`{}`), "a/wave.json": enc.encode(JSON.stringify(anim)) })
    expect((await motionDotLottie(plain)).stateMachine).toBeNull()
  })
})
