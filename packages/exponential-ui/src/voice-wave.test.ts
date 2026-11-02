import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/voice-wave.json" with { type: "json" }
import styleJson from "../catalog/style.json" with { type: "json" }
import runtimeStyle from "../generated/style.runtime.json" with { type: "json" }
import { resolveRecipe } from "./theme"
import { BUILTIN_THEMES, builtinTheme } from "./themes"
import { VOICE_WAVE, VOICE_WAVE_BARS, VOICE_WAVE_SIZES, voiceWave, voiceWaveBars, voiceWavePath, voiceWavePhase, type VoiceWaveBarsInput, type VoiceWaveInput } from "./voice-wave"
import { VOICE_WAVE_CLASSES, VOICE_WAVE_SIZE_PX, voiceWaveClock } from "./web/primitives/voice-wave"

describe(`NEW-77 voice wave`, () => {
  test(`fixtures/voice-wave.json replays`, () => {
    for (const c of fixture.cases) expect(voiceWave(c.input as VoiceWaveInput), c.name).toEqual(c.expected as never)
  })

  test(`idle and muted are circles, the path closes`, () => {
    const wave = voiceWave({ size: 200, activity: `idle`, level: 1 })
    const [outer] = wave.rings
    expect(outer!.points).toHaveLength(VOICE_WAVE.steps + 1)
    expect(outer!.points[0]).toEqual(outer!.points[VOICE_WAVE.steps]!)
    for (const [x, y] of outer!.points) expect(Math.hypot(x - 100, y - 100)).toBeCloseTo(66, 1)
    expect(voiceWavePath(outer!).endsWith(`Z`)).toBe(true)
  })

  test(`the level widens the wave, reduced motion freezes the phase`, () => {
    const spread = (level: number) => {
      const pts = voiceWave({ size: 160, activity: `listening`, level }).rings[0]!.points
      const radii = pts.map(([x, y]) => Math.hypot(x - 80, y - 80))
      return Math.max(...radii) - Math.min(...radii)
    }
    expect(spread(1)).toBeGreaterThan(spread(0) * 2)
    expect(voiceWavePhase(`listening`, 2, true)).toBe(0)
    expect(voiceWavePhase(`muted`, 2)).toBe(0)
    expect(voiceWavePhase(`thinking`, 2)).toBeCloseTo(1.8)
  })
})

describe(`VAPP-162 voice wave bars`, () => {
  test(`fixtures/voice-wave.json bars replay`, () => {
    for (const c of fixture.bars) expect(voiceWaveBars(c.input as VoiceWaveBarsInput), c.name).toEqual(c.expected as never)
  })

  test(`idle lies flat at the minimum, the box follows the size class`, () => {
    const idle = voiceWaveBars({ size: `md`, activity: `idle`, level: 1 })
    expect(idle.width).toBe(VOICE_WAVE_BARS.count * 3 + (VOICE_WAVE_BARS.count - 1) * 3)
    for (const bar of idle.bars) expect(bar.height).toBeCloseTo(24 * VOICE_WAVE_BARS.minFraction, 2)
    const loud = voiceWaveBars({ size: `md`, activity: `speaking`, level: 1 })
    expect(Math.max(...loud.bars.map((b) => b.height))).toBeGreaterThan(idle.bars[0]!.height * 3)
    expect(voiceWaveBars({ size: `md`, activity: `speaking`, level: 1, elapsed: 2, reducedMotion: true })).toEqual(voiceWaveBars({ size: `md`, activity: `speaking`, level: 1 }))
  })
})

describe(`VAPP-160 voice wave hardening`, () => {
  test(`negative levels and bands clamp to 0 (the fixture's clamp cases)`, () => {
    const names = fixture.cases.map((c) => c.name).concat(fixture.bars.map((c) => c.name))
    expect(names.filter((n) => n.includes(`negative`)).length).toBeGreaterThanOrEqual(4)
    expect(voiceWave({ size: 56, activity: `listening`, level: -0.5, elapsed: 0.5 })).toEqual(voiceWave({ size: 56, activity: `listening`, level: 0, elapsed: 0.5 }))
    expect(voiceWave({ size: 160, activity: `speaking`, level: 0.8, bands: [-0.5, 0.7, -2, 1], elapsed: 0.75 })).toEqual(voiceWave({ size: 160, activity: `speaking`, level: 0.8, bands: [0, 0.7, 0, 1], elapsed: 0.75 }))
    expect(voiceWaveBars({ size: `lg`, activity: `speaking`, level: 0.6, bands: [-0.4, 1, -3, 0.5], elapsed: 0.3 })).toEqual(voiceWaveBars({ size: `lg`, activity: `speaking`, level: 0.6, bands: [0, 1, 0, 0.5], elapsed: 0.3 }))
  })

  test(`the size table: VOICE_WAVE_SIZE_PX, the size classes and every built-in theme's VoiceWave/root agree`, () => {
    for (const size of VOICE_WAVE_SIZES) {
      const px = VOICE_WAVE_SIZE_PX[size]
      // Tailwind's spacing scale: size-N = N × 4 px.
      expect(Number(VOICE_WAVE_CLASSES.size[size].replace(`size-`, ``)) * 4, size).toBe(px)
      for (const { id } of BUILTIN_THEMES) {
        const root = resolveRecipe(builtinTheme(id)!, { component: `VoiceWave`, part: `root`, props: { size, variant: `ring`, activity: `listening` } }, `light`)
        expect([root.width, root.height], `${id} ${size}`).toEqual([px, px])
      }
    }
  })

  test(`generated/style.runtime.json = catalog/style.json without its comments`, () => {
    const strip = (v: unknown): unknown =>
      Array.isArray(v) ? v.map(strip) : v && typeof v === `object` ? Object.fromEntries(Object.entries(v).filter(([k]) => k !== `$comment`).map(([k, x]) => [k, strip(x)])) : v
    expect(runtimeStyle).toEqual(strip(styleJson) as never)
    expect(JSON.stringify(runtimeStyle)).not.toContain(`$comment`)
  })

  test(`the clock ticks only while its element is on screen`, () => {
    const g = globalThis as Record<string, unknown>
    const saved = { IntersectionObserver: g.IntersectionObserver, requestAnimationFrame: g.requestAnimationFrame, cancelAnimationFrame: g.cancelAnimationFrame }
    let frames = new Map<number, FrameRequestCallback>()
    let next = 1
    let observed: ((entries: { isIntersecting: boolean }[]) => void) | null = null
    let disconnected = false
    g.requestAnimationFrame = (cb: FrameRequestCallback) => {
      frames.set(next, cb)
      return next++
    }
    g.cancelAnimationFrame = (id: number) => frames.delete(id)
    g.IntersectionObserver = class {
      constructor(cb: (entries: { isIntersecting: boolean }[]) => void) {
        observed = cb
      }
      observe() {}
      disconnect() {
        disconnected = true
      }
    }
    const flush = (now: number) => {
      const due = [...frames.values()]
      frames = new Map()
      for (const cb of due) cb(now)
    }
    try {
      const seen: number[] = []
      const stop = voiceWaveClock((t) => seen.push(t), {} as Element)
      expect(frames.size).toBe(0) // nothing until the observer reports
      observed!([{ isIntersecting: true }])
      flush(1000)
      flush(1500)
      expect(seen).toEqual([0.5])
      observed!([{ isIntersecting: false }]) // scrolled away / display: none
      expect(frames.size).toBe(0)
      observed!([{ isIntersecting: true }])
      flush(3000)
      expect(seen).toEqual([0.5, 2]) // the wall clock kept counting
      stop()
      expect(disconnected).toBe(true)
      expect(frames.size).toBe(0)
      // Without an element (or an observer) it ticks as before.
      const plain: number[] = []
      const stopPlain = voiceWaveClock((t) => plain.push(t))
      flush(0)
      flush(250)
      expect(plain).toEqual([0.25])
      stopPlain()
      expect(frames.size).toBe(0)
    } finally {
      Object.assign(g, saved)
    }
  })
})
