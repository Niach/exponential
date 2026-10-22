import * as React from "react"
import type { LucideIcon } from "lucide-react"
import { SEGMENTED_CONTROL_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"
import { GlassTabsRow } from "./glass-rows"
import { Tabs, TabsList, TabsTrigger } from "./tabs"

interface SegmentedOption<TValue extends string> {
  value: TValue
  label: React.ReactNode
  /** A leading glyph. A concept icon, never a raw lucide import. */
  icon?: LucideIcon
  disabled?: boolean
}

interface SegmentedControlProps<TValue extends string> {
  value: TValue
  onValueChange: (value: TValue) => void
  options: readonly SegmentedOption<TValue>[]
  /** Draw as a glass group's first row instead of a floating capsule. */
  embedded?: boolean
  /** Span the container instead of the capsule's natural `w-fit`, so the
   *  segments (already `flex-1`) share it equally. For narrow slots — a side
   *  panel — where the natural width overflows and clips the last label. */
  fill?: boolean
  /** Disables every segment. */
  disabled?: boolean
  /** Extra classes on the strip (the `TabsList` / the embedded row). */
  className?: string
}

function SegmentedControl<TValue extends string>({
  value,
  onValueChange,
  options,
  embedded = false,
  fill = false,
  disabled = false,
  className,
}: SegmentedControlProps<TValue>) {
  const triggers = options.map((option) => {
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
  })

  if (embedded) {
    return (
      <GlassTabsRow
        value={value}
        onValueChange={(next) => onValueChange(next as TValue)}
        className={className}
      >
        {triggers}
      </GlassTabsRow>
    )
  }

  return (
    <Tabs
      data-slot="segmented-control"
      value={value}
      onValueChange={(next) => onValueChange(next as TValue)}
    >
      <TabsList className={cn(fill && SEGMENTED_CONTROL_CLASSES.fill, className)}>{triggers}</TabsList>
    </Tabs>
  )
}

export { SegmentedControl }
export type { SegmentedControlProps, SegmentedOption }
