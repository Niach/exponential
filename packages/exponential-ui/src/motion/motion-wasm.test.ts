// EXP-1603: the committed Motion WASM is current and replays the engine's
// display-list fixtures byte for byte (fixtures/motion-frames.json) and its
// refusals (fixtures/motion-validate.json).
import { describe, expect, it } from "bun:test"
import { readFileSync } from "node:fs"
import { motionSourceHash } from "../../scripts/motion-source-hash"
import frames from "../../fixtures/motion-frames.json"
import playback from "../../fixtures/motion-playback.json"
import stateFixture from "../../fixtures/motion-state.json"
import { motionPropsProblems, motionWasmSync } from "./server"

const wasm = motionWasmSync()

describe(`motion wasm`, () => {
  it(`was built from the current engine sources (else run scripts/build-motion-wasm.sh)`, () => {
    const built = readFileSync(new URL(`../../wasm/motion/SOURCE_HASH`, import.meta.url), `utf8`).trim()
    expect(built).toBe(motionSourceHash())
  })

  // The budget is on the RAW bytes: a gzip size depends on the zlib build
  // (Bun 1.3.10 gave 205,127 B, Bun 1.3.14 under 200 KB, Python's zlib
  // 203,927 B for the same file), so a gz budget at the edge passed in CI and
  // failed on a dev box. 480,000 B = the 465,752 B built at #2206 + ~3 %
  // (about 200 KB gzipped). A real growth past it means rebuilding with the
  // size flags (scripts/build-motion-wasm.sh) or raising this on purpose.
  it(`stays inside the 480 KB raw budget (about 200 KB gzipped)`, () => {
    const bytes = readFileSync(new URL(`../../wasm/motion/eui_motion_wasm_bg.wasm`, import.meta.url))
    expect(bytes.byteLength).toBeLessThanOrEqual(480_000)
  })

  type Case = { id: string; lottie: unknown; frames: { frame: number; packed: number[] }[] }
  for (const c of (frames as unknown as { cases: Case[] }).cases) {
    it(`replays ${c.id}`, () => {
      const player = new wasm.MotionPlayer(JSON.stringify(c.lottie), ``)
      for (const f of c.frames) {
        const got = Array.from(player.sample(f.frame))
        expect(got.length).toBe(f.packed.length)
        got.forEach((v, i) => expect(Math.abs(v - f.packed[i]!)).toBeLessThanOrEqual(1e-4 * (1 + Math.abs(f.packed[i]!))))
      }
      player.free()
    })
  }
})

describe(`motion playback (fixtures/motion-playback.json)`, () => {
  type P = { id: string; props: unknown; poster: number; progressHalf: number; ticks: { elapsedMs: number; frame: number; done: boolean }[] }
  for (const c of (playback as unknown as { cases: P[] }).cases) {
    it(c.id, () => {
      const player = new wasm.MotionPlayer(JSON.stringify(playback.lottie), ``)
      player.setPlayback(JSON.stringify(c.props))
      expect(player.posterFrame()).toBeCloseTo(c.poster, 6)
      expect(player.progressFrame(0.5)).toBeCloseTo(c.progressHalf, 6)
      for (const t of c.ticks) {
        expect(player.tick(t.elapsedMs)).toBeCloseTo(t.frame, 6)
        expect(player.done(t.elapsedMs)).toBe(t.done)
      }
      player.free()
    })
  }
})

describe(`motion state machines (fixtures/motion-state.json)`, () => {
  type Snap = { state: string; entered: string[]; final: boolean; range: [number, number]; autoplay: boolean; inputs: Record<string, unknown> }
  type Step = Snap & { step: { post?: string; layer?: string; set?: Record<string, unknown>; complete?: boolean } }
  type C = { id: string; machine: unknown; initial: Snap; steps: Step[] }
  for (const c of (stateFixture as unknown as { cases: C[] }).cases) {
    it(c.id, () => {
      const player = new wasm.MotionPlayer(JSON.stringify(stateFixture.lottie), ``)
      expect(player.setStateMachine(JSON.stringify(c.machine))).toBe(true)
      const check = (want: Snap) => {
        const got = JSON.parse(player.smStatus()) as Snap
        expect(got.state).toBe(want.state)
        expect(got.entered).toEqual(want.entered)
        expect(got.final).toBe(want.final)
        expect(got.inputs).toEqual(want.inputs)
        // The state's playback reached the clock: its range shows in the poster frame.
        expect(player.posterFrame()).toBeCloseTo(want.range[0], 6)
      }
      check(c.initial)
      for (const s of c.steps) {
        if (s.step.post) player.smPost(s.step.post, s.step.layer ?? ``)
        if (s.step.set) player.smSetInputs(JSON.stringify(s.step.set))
        if (s.step.complete) player.smPost(`complete`, ``)
        check(s)
      }
      player.free()
    })
  }
})

describe(`motionPropsProblems`, () => {
  const ball = {
    size: [100, 100],
    duration: 1000,
    layers: [{ type: `shape`, shapes: [{ type: `ellipse`, position: [50, 50], size: [20, 20] }, { type: `fill`, color: `$color.primary` }] }],
  }
  it(`accepts an inline dialect animation`, () => expect(motionPropsProblems(ball)).toEqual([]))
  it(`names dialect errors with their path`, () => {
    const bad = { ...ball, layers: [{ type: `shape`, shapes: [{ type: `blob` }] }] }
    expect(motionPropsProblems(bad)[0]).toContain(`/layers/0/shapes/0/type`)
  })
  it(`refuses expressions in inline Lottie`, () => {
    const lottie = { v: `5.7.0`, fr: 30, ip: 0, op: 30, w: 10, h: 10, layers: [{ ty: 4, ind: 1, ip: 0, op: 30, st: 0, ks: { r: { a: 0, k: 0, x: `time*10` } }, shapes: [] }] }
    expect(motionPropsProblems({ lottie }).join()).toContain(`expression`)
  })
  it(`takes src only as an attachment`, () => {
    expect(motionPropsProblems({ src: `/api/attachments/abc-123` })).toEqual([])
    expect(motionPropsProblems({ src: `https://lottiefiles.com/x.json` })[0]).toContain(`attachment`)
  })
  it(`wants exactly one source`, () => expect(motionPropsProblems({ ...ball, src: `/api/attachments/a` })[0]).toContain(`exactly one`))
})
