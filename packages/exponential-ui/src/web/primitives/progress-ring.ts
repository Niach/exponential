// EXP-1097 — the sub-issue COMPLETION ring that leads the issue detail's
// "Sub-issues" band: `done` of `total` as an arc over a 20% track. The same
// 16-unit box, 2-unit stroke and track as `ContextRing` (`ringGeometry`), so
// the two rings read as one family; drawn at icon size (14px by default) and
// painted in the team's COMPLETED status colour, handed in exactly like a
// `StatusGlyph`'s (a token class for builtins, a hex for custom rows).

/** The filled share of the ring, 0..100 (an empty or nonsensical total = 0). */
export function progressRingPercent(done: number, total: number): number {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) return 0
  return (Math.min(Math.max(done, 0), total) / total) * 100
}

export const PROGRESS_RING_CLASSES = {
  root: `shrink-0 -rotate-90`,
} as const
