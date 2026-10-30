import * as React from "react"
import { METER_CLASSES, type MeterTone } from "@exponential_app/eui/web/primitives"

import { Progress } from "./progress"
import { cn } from "./cn"

export type { MeterTone } from "@exponential_app/eui/web/primitives"

export function Meter({
  value,
  tone = `normal`,
  className,
  ...props
}: Omit<React.ComponentProps<typeof Progress>, `value`> & {
  /** 0-100, the caller clamps (`parseWindow` already does). `null` =
   *  indeterminate: work with a duration but no measurable progress. */
  value: number | null
  tone?: MeterTone
}) {
  return (
    <Progress
      data-slot="meter"
      data-tone={tone}
      value={value}
      className={cn(METER_CLASSES.root, METER_CLASSES.tones[tone], className)}
      {...props}
    />
  )
}
