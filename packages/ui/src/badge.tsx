import * as React from "react"

import { cn } from "./cn"

// EXP-962 — the COUNT badge: a 16px capsule carrying a number, the smallest
// chip there is. `Pill size="sm"` is 24 tall, which is a chip beside a label;
// this rides a nav entry's corner (the sidebar rail's drafts count) or a
// row's trailing edge, and says "how many", never what. Zero renders NOTHING:
// a badge is a signal, and an empty signal is noise. Past `max` it reads
// `99+`. PLACEMENT stays at the call site (the rail positions it absolutely
// at a row's right edge or an icon's corner) — the badge owns only its shape.
//
//   tone  muted   — the resting count (drafts you parked), muted fill
//         primary — the count that wants you (unread), accent fill

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
  tone?: `muted` | `primary`
}) {
  if (count <= 0) return null
  return (
    <span
      data-slot="badge"
      data-tone={tone}
      className={cn(
        `pointer-events-none inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full px-1 text-[10px] leading-none font-semibold tabular-nums`,
        tone === `primary`
          ? `bg-primary text-primary-foreground`
          : `bg-muted text-muted-foreground`,
        className
      )}
      {...props}
    >
      {count > max ? `${max}+` : count}
    </span>
  )
}
