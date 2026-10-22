import { Show } from "solid-js"
import { PROGRESS_RING_CLASSES, RING_RADIUS, RING_SIZE, RING_STROKE, progressRingPercent, ringGeometry } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export { progressRingPercent } from "@exponential-at/ui/web/primitives"

export function ProgressRing(props: {
  done: number
  total: number
  /** Rendered box in px. */
  size?: number
  /** A `text-*` token class — the builtin completed status. */
  colorClass?: string
  /** A synced hex — a custom completed row. Wins over `colorClass`. */
  colorHex?: string
  class?: string
}) {
  const geometry = () => ringGeometry(progressRingPercent(props.done, props.total))
  const centre = RING_SIZE / 2
  return (
    <svg
      viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
      width={props.size ?? 14}
      height={props.size ?? 14}
      aria-hidden="true"
      data-slot="progress-ring"
      data-done={props.done}
      data-total={props.total}
      class={cn(PROGRESS_RING_CLASSES.root, props.colorClass, props.class)}
      style={props.colorHex ? { color: props.colorHex } : undefined}
    >
      <circle cx={centre} cy={centre} r={RING_RADIUS} fill="none" stroke="currentColor" stroke-width={RING_STROKE} opacity={0.2} />
      <Show when={props.done > 0}>
        <circle
          data-slot="progress-ring-arc"
          cx={centre}
          cy={centre}
          r={RING_RADIUS}
          fill="none"
          stroke="currentColor"
          stroke-width={RING_STROKE}
          stroke-linecap="round"
          stroke-dasharray={String(geometry().circumference)}
          stroke-dashoffset={String(geometry().dashOffset)}
        />
      </Show>
    </svg>
  )
}
