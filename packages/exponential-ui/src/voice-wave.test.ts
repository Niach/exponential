import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/voice-wave.json" with { type: "json" }
import { VOICE_WAVE, voiceWave, voiceWavePath, voiceWavePhase, type VoiceWaveInput } from "./voice-wave"

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
