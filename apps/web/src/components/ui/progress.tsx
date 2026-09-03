import * as React from "react"
import * as ProgressPrimitive from "@radix-ui/react-progress"

import { cn } from "@/lib/utils"

function Progress({
  className,
  value,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  // EXP-724: no value = INDETERMINATE (Radix's own `null` state) — a short bar
  // sweeping the track, for work with a real duration but no measurable
  // progress (the steering compaction strip). Under reduced motion it parks
  // at the left as a static, dimmed segment rather than animating.
  const indeterminate = value === null || value === undefined
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      value={value}
      className={cn(
        `bg-primary/20 relative h-2 w-full overflow-hidden rounded-full`,
        className
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          `bg-primary h-full flex-1 transition-all`,
          indeterminate
            ? `w-1/3 motion-safe:animate-progress-indeterminate motion-reduce:opacity-60`
            : `w-full`
        )}
        style={
          indeterminate
            ? undefined
            : { transform: `translateX(-${100 - (value || 0)}%)` }
        }
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }
