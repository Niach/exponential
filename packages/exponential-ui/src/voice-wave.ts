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
  /** The one child's own laid-out size (the core centres it at that size). */
  child?: { width: number; height: number }
}

/** Where the painter draws the child: its own box scaled about the wave's
 *  centre by `scale` = min(1, childSize / its longer edge), never up; 0 =
 *  kept in the tree but not painted (`sm`). */
export interface VoiceWaveChildBox {
  x: number
  y: number
  width: number
  height: number
  scale: number
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
  /** Present when the input gave the child's size. */
  child?: VoiceWaveChildBox
}

const clamp01 = (n: number) => (Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0)
const round2 = (n: number) => Math.round(n * 100) / 100

/** The phase a painter draws at `elapsed` seconds. */
export function voiceWavePhase(raw: VoiceWaveActivity, elapsed: number, reducedMotion = false): number {
  const activity = voiceWaveActivity(raw)
  if (reducedMotion || activity === `idle` || activity === `muted`) return 0
  return (Number.isFinite(elapsed) ? Math.max(0, elapsed) : 0) * VOICE_WAVE.phaseSpeed
}

/** The child's edge for a box `size` (the size enum's px, sm 32 · md 56 · lg 160 · xl 200). */
export function voiceWaveChildSize(size: number): number {
  const v = VOICE_WAVE
  if (size < 40) return 0
  return round2(size * (size >= 100 ? v.childScaleLarge : v.childScale))
}

/** The child's painted box in a wave of edge `size` (see VoiceWaveChildBox). */
export function voiceWaveChildBox(size: number, child: { width: number; height: number }): VoiceWaveChildBox {
  const edge = voiceWaveChildSize(size)
  const w = Math.max(0, child.width)
  const h = Math.max(0, child.height)
  const longer = Math.max(w, h)
  const scale = edge === 0 ? 0 : longer <= edge || longer === 0 ? 1 : Math.round((edge / longer) * 10000) / 10000
  const width = round2(w * scale)
  const height = round2(h * scale)
  return { x: round2((size - width) / 2), y: round2((size - height) / 2), width, height, scale }
}

/** The three rings for one frame. Coordinates are rounded to 0.01 px. */
/** The activity as the catalog names it; anything else (a bound string, a
 *  typo) is `idle`, in every renderer (the Rust core: `#[serde(other)]`). */
export function voiceWaveActivity(value: unknown): VoiceWaveActivity {
  return (VOICE_WAVE_ACTIVITIES as readonly unknown[]).includes(value) ? (value as VoiceWaveActivity) : `idle`
}

export function voiceWave(raw: VoiceWaveInput): VoiceWave {
  const input = { ...raw, activity: voiceWaveActivity(raw.activity) }
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
  const out: VoiceWave = { rings, childSize: voiceWaveChildSize(size), phase: round2(phase) }
  if (input.child) out.child = voiceWaveChildBox(size, input.child)
  return out
}

/** An SVG path (`M … L … Z`) for a ring: what the web painters set as `d`. */
export function voiceWavePath(ring: VoiceWaveRing): string {
  return `${ring.points.map(([x, y], i) => `${i === 0 ? `M` : `L`}${x} ${y}`).join(` `)}Z`
}

// ---------------------------------------------------------------------------
// VAPP-162: `variant: bars`, the horizontal level meter (style.json
// `voiceWave.bars`). Same inputs as the rings; no child.
// ---------------------------------------------------------------------------

export const VOICE_WAVE_BARS = styleJson.voiceWave.bars

export const VOICE_WAVE_SIZES = [`sm`, `md`, `lg`, `xl`] as const
export type VoiceWaveSizeName = (typeof VOICE_WAVE_SIZES)[number]

export const VOICE_WAVE_VARIANTS = [`ring`, `bars`] as const
export type VoiceWaveVariant = (typeof VOICE_WAVE_VARIANTS)[number]

/** The variant as the catalog names it; anything else is `ring`. */
export function voiceWaveVariant(value: unknown): VoiceWaveVariant {
  return (VOICE_WAVE_VARIANTS as readonly unknown[]).includes(value) ? (value as VoiceWaveVariant) : `ring`
}

export interface VoiceWaveBarsInput {
  /** The size class (an unknown one is `md`). */
  size: VoiceWaveSizeName
  activity: VoiceWaveActivity
  level?: number
  bands?: readonly number[]
  elapsed?: number
  reducedMotion?: boolean
}

export interface VoiceWaveBar {
  x: number
  y: number
  width: number
  height: number
  radius: number
}

export interface VoiceWaveBars {
  /** The root box: count × barWidth + (count − 1) × gap wide, `height` tall. */
  width: number
  height: number
  phase: number
  bars: VoiceWaveBar[]
}

/** The box of a `bars` meter for a size class. */
export function voiceWaveBarsBox(raw: unknown): { width: number; height: number; barWidth: number; gap: number } {
  const size = (VOICE_WAVE_SIZES as readonly unknown[]).includes(raw) ? (raw as VoiceWaveSizeName) : `md`
  const spec = VOICE_WAVE_BARS.sizes[size]
  const n = VOICE_WAVE_BARS.count
  return { width: n * spec.barWidth + (n - 1) * spec.gap, height: spec.height, barWidth: spec.barWidth, gap: spec.gap }
}

/** The bars for one frame (style.json `voiceWave.bars` $comment). */
export function voiceWaveBars(raw: VoiceWaveBarsInput): VoiceWaveBars {
  const activity = voiceWaveActivity(raw.activity)
  const v = VOICE_WAVE
  const b = VOICE_WAVE_BARS
  const box = voiceWaveBarsBox(raw.size)
  const level = clamp01(raw.level ?? 0)
  const bands = (raw.bands ?? []).slice(0, v.maxBands).map(clamp01)
  const phase = voiceWavePhase(activity, raw.elapsed ?? 0, raw.reducedMotion)
  const full = v.ampBase + v.ampLevel
  const drive = activity === `listening` || activity === `speaking` ? (v.ampBase + v.ampLevel * level) / full : activity === `thinking` ? v.thinkingAmp / full : 0
  const flat = bands.length === 0 || activity === `thinking`
  const n = b.count
  const bars: VoiceWaveBar[] = []
  for (let i = 0; i < n; i += 1) {
    const band = flat ? 1 : bands[i % bands.length]!
    const envelope = 1 - b.envelope * Math.abs((2 * i) / (n - 1) - 1)
    const shape = 0.5 + 0.5 * Math.sin(b.shapeFreq * i + phase * (1 + (i % 3) * 0.5))
    const fraction = b.minFraction + (1 - b.minFraction) * drive * band * envelope * (b.shapeBase + (1 - b.shapeBase) * shape)
    const height = round2(box.height * fraction)
    bars.push({ x: i * (box.barWidth + box.gap), y: round2((box.height - height) / 2), width: box.barWidth, height, radius: box.barWidth / 2 })
  }
  return { width: box.width, height: box.height, phase: round2(phase), bars }
}
