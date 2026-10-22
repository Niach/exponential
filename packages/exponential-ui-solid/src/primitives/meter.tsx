import { splitProps } from "solid-js"
import { METER_CLASSES, type MeterTone } from "@exponential-at/ui/web/primitives"

import { Progress, type ProgressProps } from "./progress"
import { cn } from "./cn"

export type { MeterTone } from "@exponential-at/ui/web/primitives"

export function Meter(
  props: Omit<ProgressProps, `value`> & {
    /** 0-100, the caller clamps. `null` = indeterminate. */
    value: number | null
    tone?: MeterTone
  }
) {
  const [local, rest] = splitProps(props, [`value`, `tone`, `class`])
  const tone = () => local.tone ?? `normal`
  return <Progress data-slot="meter" data-tone={tone()} value={local.value} class={cn(METER_CLASSES.root, METER_CLASSES.tones[tone()], local.class)} {...rest} />
}
