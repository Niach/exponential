import * as React from "react"
import type { LucideIcon } from "lucide-react"

import { cn } from "./cn"
import { GlassTabsRow } from "./glass-rows"
import { SEGMENTED_TAB, Tabs, TabsList, TabsTrigger } from "./tabs"

// EXP-941 — the segmented capsule as a COMPONENT.
//
// `tabs.tsx` has owned the class recipe for a while, but the eight strips that
// use it each wired their own `Tabs` + `TabsList` + N `TabsTrigger`s by hand,
// so the triggers drifted in padding and in whether they carried a glyph. This
// renders the strip from an option array instead. It changes no string in
// `tabs.tsx`: the constants stay exactly as they are (other surfaces, and
// `mobile-detail-header.test.ts`, read them directly).
//
// `embedded` swaps the free-floating capsule for `GlassTabsRow` — the strip as
// the FIRST ROW of a glass group (EXP-694), which is what the settings sheets
// and the natives' `GlassSegmentedControl(embedded:)` draw.

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
        className={SEGMENTED_TAB}
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
      <TabsList className={cn(className)}>{triggers}</TabsList>
    </Tabs>
  )
}

export { SegmentedControl }
export type { SegmentedControlProps, SegmentedOption }
