import { forwardRef, type ComponentProps } from "react"

import { Button } from "./button"
import { cn } from "./cn"

// EXP-877: the composer footer's CONTEXT RING — a 16px radial where the
// context pill used to be, and the trigger of the usage overlay the session
// already had. The number and the tone are the SESSION's, so both come in as
// props (`lib/context-ring.ts` in the app derives them from the engine's
// usage); this only draws them. It forwards its ref so a `PopoverTrigger
// asChild` can own it.

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

/** The three tones every usage surface shares — the app maps its thresholds
 *  (`severity`, lib/agent-usage.ts) onto them, never the other way round. */
export type RingTone = `normal` | `warning` | `danger`

export const RING_TONE_CLASS: Record<RingTone, string> = {
  normal: `text-muted-foreground`,
  warning: `text-amber-500`,
  danger: `text-destructive`,
}

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
          `shrink-0`,
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
          className="-rotate-90"
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
