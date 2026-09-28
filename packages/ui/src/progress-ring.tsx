import { RING_RADIUS, RING_SIZE, RING_STROKE, ringGeometry } from "./context-ring"
import { cn } from "./cn"

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

export function ProgressRing({
  done,
  total,
  size = 14,
  colorClass,
  colorHex,
  className,
}: {
  done: number
  total: number
  /** Rendered box in px. */
  size?: number
  /** A `text-*` token class — the builtin completed status. */
  colorClass?: string
  /** A synced hex — a custom completed row. Wins over `colorClass`. */
  colorHex?: string
  className?: string
}) {
  const { circumference, dashOffset } = ringGeometry(
    progressRingPercent(done, total)
  )
  const centre = RING_SIZE / 2
  return (
    <svg
      viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
      width={size}
      height={size}
      aria-hidden
      data-slot="progress-ring"
      data-done={done}
      data-total={total}
      className={cn(`shrink-0 -rotate-90`, colorClass, className)}
      style={colorHex ? { color: colorHex } : undefined}
    >
      <circle
        cx={centre}
        cy={centre}
        r={RING_RADIUS}
        fill="none"
        stroke="currentColor"
        strokeWidth={RING_STROKE}
        opacity={0.2}
      />
      {done > 0 && (
        <circle
          data-slot="progress-ring-arc"
          cx={centre}
          cy={centre}
          r={RING_RADIUS}
          fill="none"
          stroke="currentColor"
          strokeWidth={RING_STROKE}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
        />
      )}
    </svg>
  )
}
