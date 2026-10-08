import * as React from "react"

import { Progress } from "./progress"
import { cn } from "./cn"

/** EXP-909: the ONE bar every usage surface draws — the rate-limit windows,
 * the run's context, and the tiny three-bar mini line. Before this there were
 * two: a bare `Progress` (primary fill, no tone) under the Context block and a
 * hand-rolled `<span>` track with its own tone map inside the usage cards, so
 * the same 67% was two different colours two rows apart.
 *
 * It IS `Progress` — the Radix root, its track and its indicator — with the
 * severity tone painted on the indicator (`lib/agent-usage.ts` `severity`,
 * hand-mirrored ×4 with the desktop's `usage_bar::meter`, iOS `AgentUsageTrack`
 * and Android `UsageTrack`). Height comes from `className` (`h-1` for the mini
 * line, `h-1.5` for a full window), never from a prop: the tone is the only
 * thing a caller has to think about. */
export type MeterTone = `normal` | `warning` | `danger`

const TONE: Record<MeterTone, string> = {
  normal: `[&_[data-slot=progress-indicator]]:bg-foreground/30`,
  warning: `[&_[data-slot=progress-indicator]]:bg-amber-500`,
  danger: `[&_[data-slot=progress-indicator]]:bg-destructive`,
}

export function Meter({
  value,
  tone = `normal`,
  className,
  ...props
}: Omit<React.ComponentProps<typeof Progress>, `value`> & {
  /** 0-100. The caller clamps (`parseWindow` already does). */
  value: number
  tone?: MeterTone
}) {
  return (
    <Progress
      data-slot="meter"
      data-tone={tone}
      value={value}
      className={cn(
        `h-1.5 bg-glass-stroke-strong`,
        TONE[tone],
        className
      )}
      {...props}
    />
  )
}
