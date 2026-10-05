import type { ReactNode } from "react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  PickerItemBody,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"

const CheckIcon = conceptIcon(`ui-check`)

// EXP-1183 — the status / priority pickers of the issue face. The rows are
// the picker primitive's own (`PickerItemBody`, the single arm's trailing
// `ui-check`), but the SURFACE is always the anchored menu: below 768px the
// `Picker` swaps to a fixed bottom sheet, and inside a host's content-sized
// iframe that sheet lands at the frame's bottom edge, often off screen.
export function IssueMenuPicker({
  items,
  value,
  onChange,
  trigger,
  label,
  disabled,
  onOpenChange,
}: {
  items: readonly PickerItem[]
  value: string | null
  onChange: (value: string) => void
  trigger: ReactNode
  /** What the menu picks, for the assistive tree. */
  label: string
  disabled?: boolean
  onOpenChange?: (open: boolean) => void
}) {
  return (
    <DropdownMenu modal={false} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild disabled={disabled}>
        {trigger}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-52" aria-label={label}>
        {items.map((item) => (
          <DropdownMenuItem
            key={item.value}
            disabled={item.disabled}
            onSelect={() => {
              if (item.value !== value) onChange(item.value)
            }}
          >
            <PickerItemBody item={item} />
            {item.value === value && (
              <CheckIcon aria-hidden className="ml-auto size-4 shrink-0" />
            )}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
