import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

// EXP-1029 contract — THE picker primitive (EXP-1021 implements it).
//
// One primitive per platform, typed pickers on top, the same names
// everywhere: web here, IDE `ui::picker`, iOS `ExpUI/Sources/Picker`
// (`GlassPicker`, since SwiftUI owns the bare name), Android
// `ui/components/picker`. Presentation belongs to the primitive, NEVER to
// the caller:
//
//   phone   → a bottom sheet with PLAIN rows (no cards inside the sheet);
//             multi-select marks rows by the highlight colour, no circles;
//             swipe down closes
//   pointer → a context menu / popover anchored at the trigger
//             (`MENU_SURFACE_CLASS`, the `Combobox` rows)
//
// `search` adds the filter field at the top of either surface. The trigger
// is whatever chip or button the caller wants opened — the primitive owns
// the surface, the caller owns the trigger.
//
// This file is the CONTRACT: the prop types and a stub that renders the
// trigger only. `picker.test.tsx` carries the presentation rules as a
// skipped table; `picker-contract.test.tsx` asserts every typed picker
// renders through this one.

export interface PickerItem<T extends string = string> {
  /** The stable identity of the row; also what search matches on. */
  value: T
  /** What the row reads as. A string also seeds the search keywords. */
  label: ReactNode
  /** A leading glyph — always a concept icon, never a raw lucide import. */
  icon?: LucideIcon
  /** A colour for the glyph (a board's hex, a label's dot, a status tone). */
  color?: string
  /** A muted second line or trailing note (an email, a branch age). */
  description?: ReactNode
  /** Rendered, never pickable. */
  disabled?: boolean
  /** Extra search terms when `label` is not a string (an identifier, an
   *  email). */
  keywords?: string[]
}

export type PickerMode = `single` | `multi`

interface PickerPropsBase<T extends string> {
  items: readonly PickerItem<T>[]
  /** The chip or button that opens the picker. The primitive renders it as
   *  the anchor and wires the open state; the caller styles it. */
  trigger: ReactNode
  /** A filter field at the top of the surface. */
  search?: boolean
  /** What an empty `items` (or an empty search) reads as. */
  emptyText?: string
  /** The sheet's title on phones (the popover has none). */
  mobileTitle?: string
  disabled?: boolean
  /** Controlled open state; uncontrolled when absent. */
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** Popover alignment against the trigger (pointer only). */
  align?: `start` | `end`
  className?: string
  "data-testid"?: string
}

export type PickerProps<T extends string = string> = PickerPropsBase<T> &
  (
    | {
        mode: `single`
        /** The picked value, or null while none is. */
        value: T | null
        /** Fires on a pick and closes the surface. */
        onChange: (value: T) => void
      }
    | {
        mode: `multi`
        value: readonly T[]
        /** Fires on every toggle with the whole new set; the surface stays
         *  open. */
        onChange: (value: T[]) => void
      }
  )

/**
 * THE picker. Contract stub: renders the trigger only, under the
 * `data-slot="picker"` marker every typed picker is asserted through.
 * EXP-1021 replaces the body with the two surfaces.
 */
export function Picker<T extends string = string>(props: PickerProps<T>) {
  return (
    <span
      data-slot="picker"
      data-picker-mode={props.mode}
      data-picker-search={props.search ? `true` : undefined}
      data-testid={props[`data-testid`]}
      className="contents"
    >
      {props.trigger}
    </span>
  )
}

/** The keywords a row matches on: the explicit ones, else a string label. */
export function pickerItemKeywords(item: PickerItem<string>): string[] {
  if (item.keywords && item.keywords.length > 0) return item.keywords
  return typeof item.label === `string` ? [item.label] : []
}
