// Round 1 (docs/round-1-contract.md §3, Chart): the numbers every painter
// must agree on so a chart is pixel-faithful across platforms — the value
// extent per kind and the "nice" axis ticks (Heckbert's nice numbers). Pure,
// mirrored by the Rust core and the native painters; geometry (bars, arcs)
// stays each painter's, fed by these.

export interface ChartSeries {
  name: string
  values: readonly number[]
  tone?: string
}

/** The value range the y axis spans: `min` defaults to 0 (lower when a value
 *  is negative), `max` to the largest value (at least 0) — for `stackedBar`
 *  the largest category sum of the POSITIVE values, and `min` the lowest sum
 *  of the NEGATIVE ones (positive and negative stacks grow apart from 0).
 *  Sparkline spans its own min..max. Pie/donut use the same. Non-finite
 *  values are skipped. Loops, never spreads: a 150k-point series is fine. */
export function chartExtent(kind: string, series: readonly ChartSeries[], min?: number, max?: number): { min: number; max: number } {
  let lo = Infinity
  let hi = -Infinity
  if (kind === `stackedBar`) {
    let n = 0
    for (const s of series) n = Math.max(n, s.values.length)
    for (let i = 0; i < n; i++) {
      let pos = 0
      let neg = 0
      for (const s of series) {
        const v = s.values[i]
        if (!Number.isFinite(v)) continue
        if (v > 0) pos += v
        else neg += v
      }
      if (pos > hi) hi = pos
      if (neg < lo) lo = neg
    }
  } else {
    for (const s of series)
      for (const v of s.values) {
        if (!Number.isFinite(v)) continue
        if (v < lo) lo = v
        if (v > hi) hi = v
      }
  }
  if (kind !== `sparkline`) {
    lo = Math.min(0, lo)
    hi = Math.max(0, hi)
  }
  if (!Number.isFinite(lo)) lo = 0
  if (!Number.isFinite(hi)) hi = 0
  lo = min ?? lo
  hi = max ?? hi
  if (hi <= lo) hi = lo + 1
  return { min: lo, max: hi }
}

/** The numbers a Chart's accessible name fills `$string.chartSummary` with:
 *  the series names (", "-joined) and the extent of the data (not the axis). */
export function chartSummaryParams(series: readonly ChartSeries[]): { series: string; min: number; max: number } {
  let lo = Infinity
  let hi = -Infinity
  for (const s of series)
    for (const v of s.values) {
      if (!Number.isFinite(v)) continue
      if (v < lo) lo = v
      if (v > hi) hi = v
    }
  return { series: series.map((s) => s.name).filter(Boolean).join(`, `), min: Number.isFinite(lo) ? lo : 0, max: Number.isFinite(hi) ? hi : 0 }
}

/** The numbers a sparkline's accessible name fills `$string.sparklineSummary`
 *  with: first, last, min, max of the finite values (0 when there are none). */
export function sparklineSummaryParams(values: readonly number[]): { first: number; last: number; min: number; max: number } {
  const finite = values.filter((v) => Number.isFinite(v))
  if (finite.length === 0) return { first: 0, last: 0, min: 0, max: 0 }
  let lo = Infinity
  let hi = -Infinity
  for (const v of finite) {
    if (v < lo) lo = v
    if (v > hi) hi = v
  }
  return { first: finite[0], last: finite[finite.length - 1], min: lo, max: hi }
}

function niceNum(x: number, round: boolean): number {
  const exp = Math.floor(Math.log10(x))
  const f = x / 10 ** exp
  const nf = round ? (f < 1.5 ? 1 : f < 3 ? 2 : f < 7 ? 5 : 10) : f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10
  return nf * 10 ** exp
}

/** Axis ticks over [min, max]: a nice step (1, 2 or 5 × 10^n) giving about
 *  `target` ticks, the range widened to whole steps, every tick rounded to
 *  the step's decimals. */
export function niceTicks(min: number, max: number, target = 5): { min: number; max: number; step: number; ticks: number[] } {
  const lo = Math.min(min, max)
  const hi = max === min ? min + 1 : Math.max(min, max)
  const range = niceNum(hi - lo, false)
  const step = niceNum(range / Math.max(1, target - 1), true)
  const decimals = Math.max(0, -Math.floor(Math.log10(step)))
  const fix = (v: number) => Number(v.toFixed(decimals))
  const niceMin = fix(Math.floor(lo / step) * step)
  const niceMax = fix(Math.ceil(hi / step) * step)
  const ticks: number[] = []
  for (let v = niceMin, i = 0; v <= niceMax + step / 2 && i < 100; i++, v = niceMin + i * step) ticks.push(fix(v))
  return { min: niceMin, max: niceMax, step, ticks }
}

/** The colour token of series (or slice) i: its tone's colour when set,
 *  else `$color.chart1..8` in order, wrapping. */
export function seriesColor(index: number, tone?: string): string {
  if (tone) return tone === `danger` ? `$color.destructive` : tone === `neutral` ? `$color.mutedForeground` : `$color.${tone}`
  return `$color.chart${(index % 8) + 1}`
}

/** The donut's hole as a share of its radius (pie = 0). */
export const DONUT_HOLE = 0.6
/** Rows past which Table and List window their children (catalog/layout.json). */
export { WINDOW_THRESHOLD } from "./layout"
