import type { LucideIcon } from "lucide-react"

// The ONE picker vocabulary an option menu renders (`OptionDropdownMenu`, the
// status/priority tables in the app's `lib/domain.ts`). It lives in the
// package because the menu primitive is here: keeping the interface in the app
// would make the package import the app.
export interface IssueOption<TValue extends string> {
  color: string
  // EXP-314: custom issue statuses carry a per-row hex instead of a Tailwind
  // token class. When present it wins (applied as an inline `color` style);
  // priorities and builtin statuses leave it undefined and keep `color`.
  colorHex?: string
  icon: LucideIcon
  label: string
  value: TValue
}
