import type { LucideIcon } from "lucide-react"
import type * as React from "react"
import type { ReactNode } from "react"

import { Button } from "../button"
import { cn } from "../cn"
import { GLASS_SELECT_TRIGGER } from "../glass-rows"
import { conceptIcon } from "../icons.generated"
import { Pill } from "../pill"

// EXP-1021 — the four things a picker is opened BY, in one place.
//
// `Picker` hands the trigger to the caller (a chip, a row, a word of a
// sentence, a bare button), but four of them are the app's own vocabulary
// and were previously private to `Combobox`'s `triggerVariant`. They live
// here so the two arms draw ONE set: `Combobox` renders these for its
// `triggerVariant`, and a `Picker` call site passes `<PickerTrigger …/>` as
// its `trigger`.
//
//   pill    the property chip (an issue's assignee, a board filter)
//   field   a full-width form row inside a dialog
//   row     the glass form ladder's picker row (label leading, value
//           trailing at 70%, chevron at 50% — the iOS `GlassPickerRow` /
//           desktop `surface::glass_picker_row` twin)
//   inline  one WORD inside a muted sentence (the composer's options line)

const ChevronGlyph = conceptIcon(`ui-chevron-down`)
const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)

/** The `row` trigger: the glass form ladder's picker row (glass-rows.tsx's
 *  `GLASS_PICKER_ROW` is the same shell on a stock `SelectTrigger`). */
export const PICKER_ROW_TRIGGER = `flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-fast outline-none hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`

/** The `inline` trigger and its collapsed word share the row of the sentence
 *  they sit in — no chrome, the sentence's own colour, hover lifts it. */
export const PICKER_INLINE_WORD = `flex items-center gap-1`
export const PICKER_INLINE_TRIGGER = `${PICKER_INLINE_WORD} outline-none hover:text-foreground focus-visible:text-foreground disabled:pointer-events-none disabled:opacity-50`

export type PickerTriggerVariant = `pill` | `field` | `row` | `inline`

export interface PickerTriggerProps {
  variant?: PickerTriggerVariant
  /** What the trigger is FOR — the `row` variant leads with it, `inline`
   *  names its word for the assistive tree, the others fall back to it when
   *  nothing is picked. */
  label: string
  /** What is picked, as a node (an action's glyph flows with its name).
   *  Absent = nothing is picked. */
  value?: ReactNode
  /** The word while nothing is picked; defaults to `label`. */
  placeholder?: string
  /** A leading glyph for the `inline` word. */
  icon?: LucideIcon
  disabled?: boolean
  className?: string
}

/** `None` / `a` / `a, b` / `a, b +3` — the one summary every trigger shows. */
export function pickerSummary(labels: string[], fallback: string) {
  if (labels.length === 0) return fallback
  if (labels.length <= 2) return labels.join(`, `)
  return `${labels.slice(0, 2).join(`, `)} +${labels.length - 2}`
}

/** One of the four triggers, as ONE element (every surface wraps it
 *  `asChild`). */
export function PickerTrigger({
  variant = `pill`,
  label,
  value,
  placeholder,
  icon: Glyph,
  disabled = false,
  className,
  ...rest
}: PickerTriggerProps &
  // `value` here is a NODE, not a form value — the DOM button's own `value`
  // would collide with it, and the surface never sets one.
  Omit<
    React.ComponentProps<`button`>,
    `value` | `children` | `disabled` | `className`
  >) {
  const nothingPicked = value === undefined || value === null
  const fallback = placeholder ?? label

  if (variant === `inline`) {
    return (
      <button
        type="button"
        // The surface wrapping this `asChild` (Radix `Slot`) hands its own
        // props down; they go FIRST so the trigger's identity — its
        // `data-slot`, its classes — is not overwritten by the popover's.
        {...rest}
        data-slot="combobox-inline-trigger"
        disabled={disabled}
        className={cn(PICKER_INLINE_TRIGGER, className)}
        title={label}
        aria-label={label}
      >
        {Glyph && <Glyph aria-hidden className="size-3.5 shrink-0" />}
        {nothingPicked ? fallback : value}
        <ChevronGlyph aria-hidden className="size-3 shrink-0" />
      </button>
    )
  }

  if (variant === `row`) {
    return (
      <button
        type="button"
        {...rest}
        data-slot="glass-picker-row"
        disabled={disabled}
        className={cn(PICKER_ROW_TRIGGER, className)}
      >
        <span className="shrink-0 text-sm text-foreground">{label}</span>
        <span className="ml-auto min-w-0 truncate text-sm text-foreground/70 [&_svg]:inline">
          {nothingPicked ? placeholder : value}
        </span>
        <ChevronRightGlyph
          aria-hidden
          className="size-3.5 shrink-0 text-foreground/50"
        />
      </button>
    )
  }

  if (variant === `field`) {
    return (
      <Button
        type="button"
        {...rest}
        variant="outline"
        size="sm"
        disabled={disabled}
        className={cn(
          // EXP-993: `overflow-hidden` + a growing, shrinkable summary — a
          // long pull-request title used to spill out of the composer's PR
          // field on both sides instead of truncating inside it.
          `h-8 w-full justify-between overflow-hidden font-normal`,
          GLASS_SELECT_TRIGGER,
          nothingPicked && `text-muted-foreground`,
          className
        )}
      >
        <span className="min-w-0 flex-1 truncate text-left">
          {nothingPicked ? fallback : value}
        </span>
        <ChevronGlyph aria-hidden className="size-3.5 shrink-0 opacity-50" />
      </Button>
    )
  }

  return (
    <Pill
      {...rest}
      mode="action"
      disabled={disabled}
      className={cn(`max-w-full`, className)}
    >
      <span className="min-w-0 truncate">
        {nothingPicked ? fallback : value}
      </span>
      <ChevronGlyph aria-hidden className="shrink-0 opacity-50" />
    </Pill>
  )
}
