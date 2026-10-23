import * as React from "react"
import { Progress as ProgressPrimitive } from "radix-ui"
import { PROGRESS_CLASSES, progressIndicatorTransform } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

function Progress({
  className,
  value,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  const indeterminate = value === null || value === undefined
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      value={value}
      className={cn(PROGRESS_CLASSES.root, className)}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          PROGRESS_CLASSES.indicator,
          indeterminate
            ? PROGRESS_CLASSES.indicatorIndeterminate
            : PROGRESS_CLASSES.indicatorDeterminate
        )}
        style={
          indeterminate
            ? undefined
            : { transform: progressIndicatorTransform(value) }
        }
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }
