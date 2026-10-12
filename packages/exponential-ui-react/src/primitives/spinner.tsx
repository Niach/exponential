import type * as React from "react"
import { LoaderCircle } from "lucide-react"

import { cn } from "./cn"

// EXP-1275 (EXP-1259 D8.1): THE inline spinner, feedback-states.json
// `inline.spinner` (10px) by default. The EXP-1274 list-head glyph (the
// GlassSectionHeader `loading` arm) IS this primitive; a pending Button swaps
// its icon slot for it at `pending.spinner` (12px). ×4: desktop
// `feedback::spinner`, iOS `ExpSpinner`, Android `Spinner`.
function Spinner({
  size = 10,
  label = `Loading`,
  className,
  ...props
}: Omit<React.ComponentProps<`svg`>, `children`> & {
  size?: number
  /** The accessible name; the glyph itself never carries words. */
  label?: string
  className?: string
}) {
  return (
    <LoaderCircle
      data-slot="spinner"
      data-testid="feedback-spinner"
      aria-label={label}
      width={size}
      height={size}
      style={{ width: size, height: size }}
      className={cn(`shrink-0 animate-spin text-muted-foreground`, className)}
      {...props}
    />
  )
}

export { Spinner }
