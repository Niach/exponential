// Group D (genUI round 4): the Ring arcs (`sweep`, `rings`) and the
// AudioPlayer waveform (`peaks`), pure geometry every painter draws from.
// fixtures/data-viz.json locks both; the Rust core (`data_viz`), Swift and
// Kotlin replay it.

/** One drawn ring: its centre-line radius, the filled fraction of the arc, its tone. */
export interface RingArc {
  r: number
  fraction: number
  tone: string
}

export interface RingArcs {
  /** Degrees clockwise from 12 o'clock where every arc starts (RTL mirrors). */
  start: number
  /** The track's length in degrees; a fill covers `sweep * fraction`. */
  sweep: number
  rings: RingArc[]
}

export interface RingInput {
  size: number
  stroke: number
  value?: unknown
  tone?: unknown
  sweep?: unknown
  rings?: unknown
}

const round2 = (n: number) => Math.round(n * 100) / 100
const finite = (v: unknown, d: number) => (typeof v === `number` && Number.isFinite(v) ? v : d)
const clamp01 = (n: number) => Math.max(0, Math.min(1, n))

/** The Ring's arcs: `sweep` (default 360 = from 12 o'clock; less = centred
 *  on the top, open at the bottom), `rings` (outermost first, each
 *  `value / max`, max default 1) else the one `value`; every ring is
 *  `stroke` wide, the gap between rings = `stroke`; a ring whose radius
 *  falls under `stroke` is dropped. A tone defaults to the Ring's, else primary. */
export function ringArcs(input: RingInput): RingArcs {
  const sweep = Math.max(0, Math.min(360, finite(input.sweep, 360)))
  const start = sweep >= 360 ? 0 : 180 + (360 - sweep) / 2
  const tone = typeof input.tone === `string` && input.tone ? input.tone : `primary`
  const list = Array.isArray(input.rings) && input.rings.length > 0 ? input.rings : [{ value: input.value, max: 1 }]
  const rings: RingArc[] = []
  list.forEach((item, i) => {
    const o = (item && typeof item === `object` ? item : {}) as Record<string, unknown>
    const r = input.size / 2 - input.stroke / 2 - i * input.stroke * 2
    if (r < input.stroke) return
    const max = finite(o.max, 1)
    const fraction = max > 0 ? clamp01(finite(o.value, 0) / max) : 0
    rings.push({ r: round2(r), fraction: Math.round(fraction * 10000) / 10000, tone: typeof o.tone === `string` && o.tone ? o.tone : tone })
  })
  return { start: round2(start), sweep: round2(sweep), rings }
}

/** The waveform's locked bar geometry (px). */
export const WAVEFORM = { barWidth: 2, gap: 2, minHeight: 2, height: 24 } as const

export interface WaveBar {
  x: number
  y: number
  width: number
  height: number
  played: boolean
}

/** The AudioPlayer waveform in a `width` × WAVEFORM.height box: as many
 *  `barWidth` bars `gap` apart as fit, centred; `peaks` (0..1) resampled
 *  to that count (a bucket's max when there are more, the nearest when
 *  fewer); a bar is at least `minHeight`, centred vertically; `played` =
 *  its centre at or before `progress * width`. No peaks = no bars. */
export function waveformBars(input: { width: number; peaks: unknown; progress?: number }): WaveBar[] {
  const peaks = Array.isArray(input.peaks) ? input.peaks.map((p) => clamp01(finite(p, 0))) : []
  const { barWidth, gap, minHeight, height } = WAVEFORM
  const n = peaks.length === 0 ? 0 : Math.max(0, Math.floor((input.width + gap) / (barWidth + gap)))
  if (n === 0) return []
  const x0 = (input.width - (n * (barWidth + gap) - gap)) / 2
  const at = clamp01(finite(input.progress, 0)) * input.width
  const bars: WaveBar[] = []
  for (let i = 0; i < n; i++) {
    let peak: number
    if (peaks.length > n) {
      const from = Math.floor((i * peaks.length) / n)
      const to = Math.max(from + 1, Math.floor(((i + 1) * peaks.length) / n))
      peak = Math.max(...peaks.slice(from, to))
    } else peak = peaks[Math.floor((i * peaks.length) / n)]!
    const h = Math.max(minHeight, peak * height)
    const x = x0 + i * (barWidth + gap)
    bars.push({ x: round2(x), y: round2((height - h) / 2), width: barWidth, height: round2(h), played: at > 0 && x + barWidth / 2 <= at })
  }
  return bars
}

/** A press or drag at `x` on a `width` track → the seek fraction 0..1. */
export function seekFraction(x: number, width: number): number {
  return width > 0 ? clamp01(x / width) : 0
}

/** An SVG path `d` for a clockwise arc of radius `r` around (`c`, `c`) from
 *  `start` over `sweep` degrees (0 = 12 o'clock); a full turn = two halves. */
export function arcPath(c: number, r: number, start: number, sweep: number): string {
  const pt = (deg: number) => {
    const a = (deg * Math.PI) / 180
    return `${round2(c + r * Math.sin(a))} ${round2(c - r * Math.cos(a))}`
  }
  if (sweep >= 360) return `M ${pt(start)} A ${r} ${r} 0 1 1 ${pt(start + 180)} A ${r} ${r} 0 1 1 ${pt(start + 360)}`
  return `M ${pt(start)} A ${r} ${r} 0 ${sweep > 180 ? 1 : 0} 1 ${pt(start + sweep)}`
}
