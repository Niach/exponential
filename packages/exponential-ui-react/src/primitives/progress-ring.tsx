import { PROGRESS_RING_CLASSES, RING_RADIUS, RING_SIZE, RING_STROKE, progressRingPercent, ringGeometry } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export { progressRingPercent } from "@exponential-at/ui/web/primitives"

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
      className={cn(PROGRESS_RING_CLASSES.root, colorClass, className)}
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
