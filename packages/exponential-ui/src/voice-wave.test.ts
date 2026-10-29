import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/voice-wave.json" with { type: "json" }
import { VOICE_WAVE, VOICE_WAVE_BARS, voiceWave, voiceWaveBars, voiceWavePath, voiceWavePhase, type VoiceWaveBarsInput, type VoiceWaveInput } from "./voice-wave"

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
