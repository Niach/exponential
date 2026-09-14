import { severity } from "@/lib/agent-usage"

// EXP-877: the composer footer's CONTEXT RING — the 16px radial that replaced
// the context pill. It is the same number the pill drew (`contextPercent`,
// lib/agent-usage.ts) and the same three tones every usage surface uses
// (`severity`: amber from 75%, destructive from 95%) — only the shape is new,
// so no threshold and no percent rule is restated here.
//
// Pure geometry + tone, no React: `components/context-ring.tsx` draws it.

/** The ring's box, in px — a footer glyph, the size of a small icon. */
export const RING_SIZE = 16
/** Stroke width of both the track and the progress arc. */
export const RING_STROKE = 2
/** Radius of the stroked circle: the box minus one stroke, halved, so the
 *  stroke sits fully inside the box (`(16 - 2) / 2 = 7`… minus a hair, so the
 *  arc's round join never clips). */
export const RING_RADIUS = 6

export interface RingGeometry {
  /** The full circle's length — the dash array. */
  circumference: number
  /** How much of it to hide: the UNfilled remainder. */
  dashOffset: number
}

/** The arc for `percent`, clamped to 0..100 (a NaN reads as empty). */
export function ringGeometry(percent: number): RingGeometry {
  const circumference = 2 * Math.PI * RING_RADIUS
  const clamped = Number.isFinite(percent)
    ? Math.min(100, Math.max(0, percent))
    : 0
  return {
    circumference,
    dashOffset: circumference * (1 - clamped / 100),
  }
}

/** The ring's colour — muted while there is room, amber at the warning
 *  threshold, destructive at the danger one (`severity`). */
export function ringToneClass(percent: number): string {
  switch (severity(percent)) {
    case `danger`:
      return `text-destructive`
    case `warning`:
      return `text-amber-500`
    default:
      return `text-muted-foreground`
  }
}
