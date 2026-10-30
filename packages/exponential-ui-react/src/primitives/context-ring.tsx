import { forwardRef, type ComponentProps } from "react"
import { CONTEXT_RING_CLASSES, RING_RADIUS, RING_SIZE, RING_STROKE, RING_TONE_CLASS, ringGeometry, type RingTone } from "@exponential_app/eui/web/primitives"

import { Button } from "./button"
import { cn } from "./cn"

export { RING_RADIUS, RING_SIZE, RING_STROKE, RING_TONE_CLASS, ringGeometry } from "@exponential_app/eui/web/primitives"
export type { RingGeometry, RingTone } from "@exponential_app/eui/web/primitives"

interface ContextRingProps extends ComponentProps<typeof Button> {
  /** How full the context window is — null while nothing measured it. */
  percent: number | null | undefined
  /** Which of the three tones to paint. Default: `normal`. */
  tone?: RingTone
  /** Draw the empty track anyway (the run has other usage worth opening).
   *  Without it, a run with no context window renders NOTHING. */
  showEmpty?: boolean
}

export const ContextRing = forwardRef<HTMLButtonElement, ContextRingProps>(
  function ContextRing(
    { percent, tone, showEmpty = false, className, ...props },
    ref
  ) {
    const value = percent ?? null
    if (value === null && !showEmpty) return null
    const { circumference, dashOffset } = ringGeometry(value ?? 0)
    return (
      <Button
        ref={ref}
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label="Usage"
        title="Usage"
        data-testid="session-context-ring"
        className={cn(
          CONTEXT_RING_CLASSES.root,
          value === null
            ? RING_TONE_CLASS.normal
            : RING_TONE_CLASS[tone ?? `normal`],
          className
        )}
        {...props}
      >
        <svg
          viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
          width={RING_SIZE}
          height={RING_SIZE}
          aria-hidden
          className={CONTEXT_RING_CLASSES.svg}
        >
          <circle
            cx={RING_SIZE / 2}
            cy={RING_SIZE / 2}
            r={RING_RADIUS}
            fill="none"
            stroke="currentColor"
            strokeWidth={RING_STROKE}
            opacity={0.2}
          />
          {value !== null && (
            <circle
              cx={RING_SIZE / 2}
              cy={RING_SIZE / 2}
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
      </Button>
    )
  }
)
