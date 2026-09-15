import { forwardRef, type ComponentProps } from "react"

import type { SessionUsageState } from "@/lib/agent-feed"
import { contextPercent } from "@/lib/agent-usage"
import {
  RING_RADIUS,
  RING_SIZE,
  RING_STROKE,
  ringGeometry,
  ringToneClass,
} from "@/lib/context-ring"
import { cn } from "@/lib/utils"
import { Button } from "@exp/ui"

// EXP-877: the composer footer's CONTEXT RING — a 16px radial where the
// context pill used to be, and the trigger of the usage overlay the session
// already had. The number and the tones are shared (`lib/context-ring.ts`);
// this only draws them. It forwards its ref so a `PopoverTrigger asChild` can
// own it.

interface ContextRingProps extends ComponentProps<typeof Button> {
  /** The engine's last context measurement — null while it measured none. */
  usage: SessionUsageState | null | undefined
  /** Draw the empty track anyway (the run has other usage worth opening).
   *  Without it, a run with no context window renders NOTHING. */
  showEmpty?: boolean
}

export const ContextRing = forwardRef<HTMLButtonElement, ContextRingProps>(
  function ContextRing({ usage, showEmpty = false, className, ...props }, ref) {
    const percent = contextPercent(usage)
    if (percent === null && !showEmpty) return null
    const { circumference, dashOffset } = ringGeometry(percent ?? 0)
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
          percent === null ? `text-muted-foreground` : ringToneClass(percent),
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
          {percent !== null && (
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
