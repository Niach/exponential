// NEW-77: the VoiceWave native (catalog/style.json `voiceWave`): three closed
// polar waves around one optional centre child, driven by a microphone or
// speaker level. Pure; the Rust core's `voice_wave` ports it and
// fixtures/voice-wave.json locks the points every painter draws.

import styleJson from "../catalog/style.json" with { type: "json" }

export const VOICE_WAVE = styleJson.voiceWave

export const VOICE_WAVE_ACTIVITIES = [`idle`, `listening`, `thinking`, `speaking`, `muted`] as const
export type VoiceWaveActivity = (typeof VOICE_WAVE_ACTIVITIES)[number]

export const VOICE_WAVE_RINGS = [`outer`, `mid`, `inner`] as const
export type VoiceWaveRingName = (typeof VOICE_WAVE_RINGS)[number]

export interface VoiceWaveInput {
  /** The box edge in px (the root recipe's width). */
  size: number
  activity: VoiceWaveActivity
  /** 0..1; clamped. */
  level?: number
  /** 0..1 each; the first `maxBands` count, clamped. */
  bands?: readonly number[]
  /** Seconds since the wave started animating (ignored when idle or muted). */
  elapsed?: number
  reducedMotion?: boolean
}

export interface VoiceWaveRing {
  /** The recipe part that colours it. */
  part: VoiceWaveRingName
  /** `steps + 1` points, x/y in px from the box's top-left (the last repeats the first). */
  points: Array<[number, number]>
  fillAlpha: number
  strokeAlpha: number
  strokeWidth: number
}

export interface VoiceWave {
  rings: VoiceWaveRing[]
  /** The centre child's edge in px; 0 = the child is not painted (`sm`). */
  childSize: number
  /** The phase the rings were drawn at (radians). */
  phase: number
}

const clamp01 = (n: number) => (Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0)
const round2 = (n: number) => Math.round(n * 100) / 100

/** The phase a painter draws at `elapsed` seconds. */
export function voiceWavePhase(activity: VoiceWaveActivity, elapsed: number, reducedMotion = false): number {
  if (reducedMotion || activity === `idle` || activity === `muted`) return 0
  return (Number.isFinite(elapsed) ? Math.max(0, elapsed) : 0) * VOICE_WAVE.phaseSpeed
}

/** The child's edge for a box `size` (the size enum's px, sm 32 · md 56 · lg 160 · xl 200). */
export function voiceWaveChildSize(size: number): number {
  const v = VOICE_WAVE
  if (size < 40) return 0
  return round2(size * (size >= 100 ? v.childScaleLarge : v.childScale))
}

/** The three rings for one frame. Coordinates are rounded to 0.01 px. */
export function voiceWave(input: VoiceWaveInput): VoiceWave {
  const v = VOICE_WAVE
  const size = Math.max(0, input.size)
  const c = size / 2
  const level = clamp01(input.level ?? 0)
  const bands = (input.bands ?? []).slice(0, v.maxBands).map(clamp01)
  const phase = voiceWavePhase(input.activity, input.elapsed ?? 0, input.reducedMotion)
  const drive =
    input.activity === `listening` || input.activity === `speaking`
      ? v.ampBase + v.ampLevel * level
      : input.activity === `thinking`
        ? v.thinkingAmp
        : 0
  const flat = bands.length === 0 || input.activity === `thinking`
  const rings = VOICE_WAVE_RINGS.map((part, k): VoiceWaveRing => {
    const harmonics = v.harmonics[k]!
    const amp = (v.ringAmp[k]! * size * drive) / 200
    const points: Array<[number, number]> = []
    for (let i = 0; i <= v.steps; i += 1) {
      const t = (i / v.steps) * Math.PI * 2
      let r = size * v.radius[k]!
      harmonics.forEach((h, j) => {
        const band = flat ? 1 : bands[(3 * k + j) % bands.length]!
        r += (amp / (j + 1)) * band * Math.sin(h * t + (v.phase0[k]! + phase) * (j + 1))
      })
      points.push([round2(c + r * Math.cos(t)), round2(c + r * Math.sin(t))])
    }
    return {
      part,
      points,
      fillAlpha: v.fillAlpha[k]!,
      strokeAlpha: v.strokeAlpha[k]!,
      strokeWidth: round2(Math.max(1, (v.strokeWidth[k]! * size) / 200)),
    }
  })
  return { rings, childSize: voiceWaveChildSize(size), phase: round2(phase) }
}

/** An SVG path (`M … L … Z`) for a ring: what the web painters set as `d`. */
export function voiceWavePath(ring: VoiceWaveRing): string {
  return `${ring.points.map(([x, y], i) => `${i === 0 ? `M` : `L`}${x} ${y}`).join(` `)}Z`
}
