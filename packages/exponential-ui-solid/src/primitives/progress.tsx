import { splitProps, type ComponentProps } from "solid-js"
import { PROGRESS_CLASSES, progressIndicatorTransform } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export type ProgressProps = ComponentProps<`div`> & {
  /** 0..max; `null`/`undefined` = indeterminate. */
  value?: number | null
  max?: number
  getValueLabel?: (value: number, max: number) => string
}

const isNumber = (v: unknown): v is number => typeof v === `number`
const validMax = (max: unknown): max is number => isNumber(max) && !Number.isNaN(max) && max > 0
const validValue = (value: unknown, max: number): value is number => isNumber(value) && !Number.isNaN(value) && value <= max && value >= 0
const progressState = (value: number | null, max: number) => (value == null ? `indeterminate` : value === max ? `complete` : `loading`)

// Radix's Progress as plain DOM (the root's aria-value* + role, the
// `data-state`/`data-value`/`data-max` on root and indicator), dressed as
// shadcn's.
function Progress(props: ProgressProps) {
  const [local, rest] = splitProps(props, [`class`, `value`, `max`, `getValueLabel`])
  const max = () => (validMax(local.max) ? local.max : 100)
  const value = () => (validValue(local.value, max()) ? (local.value as number) : null)
  const label = () => {
    const v = value()
    return v === null ? undefined : (local.getValueLabel ?? ((n: number, m: number) => `${Math.round((n / m) * 100)}%`))(v, max())
  }
  const indeterminate = () => local.value === null || local.value === undefined
  return (
    <div
      aria-valuemax={max()}
      aria-valuemin={0}
      aria-valuenow={value() ?? undefined}
      aria-valuetext={label()}
      role="progressbar"
      data-state={progressState(value(), max())}
      data-value={value() ?? undefined}
      data-max={max()}
      data-slot="progress"
      class={cn(PROGRESS_CLASSES.root, local.class)}
      {...rest}
    >
      <div
        data-state={progressState(value(), max())}
        data-value={value() ?? undefined}
        data-max={max()}
        data-slot="progress-indicator"
        class={cn(PROGRESS_CLASSES.indicator, indeterminate() ? PROGRESS_CLASSES.indicatorIndeterminate : PROGRESS_CLASSES.indicatorDeterminate)}
        style={indeterminate() ? undefined : { transform: progressIndicatorTransform(local.value) }}
      />
    </div>
  )
}

export { Progress }
