// EXP-1275 / EXP-1271 (EXP-1259 D8.4): "the mark" — Exponential's disc whose
// three cut stripes sweep in turn while a run starts. Vector only, ×4 from the
// logo's own bezier data; no Lottie, no GIF. This is the pure TIMELINE every
// client paints from, byte-locked by `fixtures/feedback-states.json`
// (`mark.cases`): desktop `ui::feedback::mark`, iOS `ExpUI FeedbackMark`,
// Android `components/FeedbackMark.kt`, web `@exp/ui` `<FeedbackMark>`.

import fixture from "../fixtures/feedback-states.json" with { type: "json" }

export const FEEDBACK_STATES = fixture
export const MARK = fixture.mark

/** A visible part of one stripe's cut, in the cubic's parameter t (0..1). */
export type MarkSpan = [number, number] | null

function easeInOutCubic(x: number): number {
  return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2
}

function round(value: number): number {
  return Math.round(value * 10_000) / 10_000
}

/**
 * The visible span of stripe `index` (0 = lower-left) at loop `phase` (0..1):
 * the cut DRAWS IN from t=0 during its draw window, stays whole, then ERASES
 * from t=0 during its erase window, and is absent outside them, so the loop
 * starts and ends on the plain disc. Rounded to 4 decimals.
 */
export function markStripeSpan(index: number, phase: number): MarkSpan {
  const stripe = MARK.stripes[index]
  if (!stripe) return null
  const p = ((phase % 1) + 1) % 1
  const [drawFrom, drawTo] = stripe.draw
  const [eraseFrom, eraseTo] = stripe.erase
  if (p < drawFrom || p >= eraseTo) return null
  if (p < drawTo) return [0, round(easeInOutCubic((p - drawFrom) / (drawTo - drawFrom)))]
  if (p < eraseFrom) return [0, 1]
  return [round(easeInOutCubic((p - eraseFrom) / (eraseTo - eraseFrom))), 1]
}

/** A point on a stripe's cubic at parameter t, in the logo's 100×100 grid. */
export function markStripePoint(index: number, t: number): [number, number] {
  const [p0, p1, p2, p3] = MARK.stripes[index].cubic as unknown as [number, number][]
  const u = 1 - t
  const a = u * u * u
  const b = 3 * u * u * t
  const c = 3 * u * t * t
  const d = t * t * t
  return [
    a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
    a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
  ]
}
