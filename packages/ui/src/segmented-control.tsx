import { SEGMENTED_CONTROL_CLASSES } from "@exponential_app/ui/web/primitives"
import {
  SegmentedControl as BaseSegmentedControl,
  type SegmentedControlProps as BaseSegmentedControlProps,
  type SegmentedOption,
} from "@exponential_app/ui-react/primitives/segmented-control"

import { GlassTabsRow } from "./glass-rows"
import { TabsTrigger } from "./tabs"

// Exponential UI's segmented capsule, plus the app's `embedded` arm: the strip
// as a glass group's first row (`GlassTabsRow`, which stays out of the SDK).

interface SegmentedControlProps<TValue extends string> extends BaseSegmentedControlProps<TValue> {
  /** Draw as a glass group's first row instead of a floating capsule. */
  embedded?: boolean
}

function SegmentedControl<TValue extends string>({ embedded = false, ...props }: SegmentedControlProps<TValue>) {
  if (!embedded) return <BaseSegmentedControl {...props} />
  const { value, onValueChange, options, disabled = false, className } = props
  return (
    <GlassTabsRow value={value} onValueChange={(next) => onValueChange(next as TValue)} className={className}>
      {options.map((option) => {
        const Glyph = option.icon
        return (
          <TabsTrigger
            key={option.value}
            value={option.value}
            disabled={disabled || option.disabled}
            className={SEGMENTED_CONTROL_CLASSES.trigger}
          >
            {Glyph ? <Glyph aria-hidden /> : null}
            {option.label}
          </TabsTrigger>
        )
      })}
    </GlassTabsRow>
  )
}

export { SegmentedControl }
export type { SegmentedControlProps, SegmentedOption }
