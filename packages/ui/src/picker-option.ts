import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

// EXP-941 — the ONE option shape every picker on the web speaks.
//
// Four incompatible option types grew side by side (`IssueOption`,
// `GlassPickerOption`, and two anonymous `{ id, name }` shapes inside the
// launch dialog), so a row rendered in one picker could not be handed to
// another. This is their union: `value` + `label` are the only required
// fields, everything else is a slot a row MAY draw. `Combobox`,
// `OptionDropdownMenu` and the glass picker rows all render from it.
//
// It is deliberately presentational — no ids, no records, no async state. A
// caller maps its own domain rows into these once, at the call site.
export interface PickerOption<TValue extends string = string> {
  /** The stable identity of the row; also what `cmdk` filters and matches on. */
  value: TValue
  /** What the row reads as. A string also seeds the default search keywords. */
  label: ReactNode
  /** Extra search terms (an email beside a name, an identifier beside a title).
   *  When omitted a string `label` is used as the only keyword. */
  keywords?: string[]
  /** A leading glyph. Always a concept icon — never a raw lucide import. */
  icon?: LucideIcon
  /** A Tailwind text colour class for the glyph (builtin statuses, priorities). */
  color?: string
  /** A per-row hex that WINS over `color` (custom statuses, EXP-314). */
  colorHex?: string
  /** A 6px coloured disc before the label (issue labels). */
  dot?: string
  /** A muted trailing note: `default`, a branch's age, a blocked reason. */
  hint?: ReactNode
  /** Rendered, never pickable. */
  disabled?: boolean
  /** Multi arm only (EXP-957): what THIS row reads as, when membership in the
   *  picker's `value` is not the whole story — a bulk edit over several
   *  issues marks a label on all of them `true`, on some `"indeterminate"`.
   *  Omitted = derived from `value`. */
  checked?: boolean | `indeterminate`
}
