import type { LucideIcon } from "lucide-react"

import type { PickerOption } from "./picker-option"

// The ONE picker vocabulary an option menu renders (`OptionDropdownMenu`, the
// status/priority tables in the app's `lib/domain.ts`). It lives in the
// package because the menu primitive is here: keeping the interface in the app
// would make the package import the app.
//
// EXP-941: it is now `PickerOption` with three of its optional slots made
// REQUIRED — a status or priority row always has a glyph, a colour and a plain
// string label, so the menu never has to fall back. Everything else
// (`keywords`, `hint`, `dot`, `disabled`) comes from the shared shape, which
// is why the same row can also be handed to `Combobox`.
export interface IssueOption<TValue extends string>
  extends PickerOption<TValue> {
  color: string
  // EXP-314: custom issue statuses carry a per-row hex instead of a Tailwind
  // token class. When present it wins (applied as an inline `color` style);
  // priorities and builtin statuses leave it undefined and keep `color`.
  colorHex?: string
  icon: LucideIcon
  label: string
}
