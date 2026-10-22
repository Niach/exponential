import * as React from "react"
import { BADGE_CLASSES, badgeLabel, type BadgeTone } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export function Badge({
  count,
  max = 99,
  tone = `muted`,
  className,
  ...props
}: Omit<React.ComponentProps<`span`>, `children`> & {
  count: number
  /** Past this the badge reads `${max}+`. */
  max?: number
  tone?: BadgeTone
}) {
  if (count <= 0) return null
  return (
    <span
      data-slot="badge"
      data-tone={tone}
      className={cn(BADGE_CLASSES.root, BADGE_CLASSES.tones[tone], className)}
      {...props}
    >
      {badgeLabel(count, max)}
    </span>
  )
}
