import type { ReactNode } from "react"

import {
  Picker,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-1029 contract — the priority picker: contract `issuePriority` in its
// order, each by the priority glyph in its tone. The app hands in its
// `ISSUE_PRIORITY_OPTIONS`-shaped rows (`lib/domain.ts`); the picker never
// owns the table.

export interface PriorityPickerOption {
  /** Contract `issuePriority`. */
  value: string
  label: string
  icon?: PickerItem[`icon`]
  /** A Tailwind text colour class or a hex for the glyph. */
  color?: string
}

interface PriorityPickerBase extends PickerSurfaceProps {
  options: readonly PriorityPickerOption[]
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  disabled?: boolean
  className?: string
}

export type PriorityPickerProps = PriorityPickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (priority: string) => void }
    | {
        mode: `multi`
        value: readonly string[]
        onChange: (priorities: string[]) => void
        /** At the cap the unpicked rows go disabled; picked ones still
         *  toggle off (the automation trigger's ten-id filters). */
        max?: number
      }
  )

export function priorityPickerItems(options: readonly PriorityPickerOption[]): PickerItem[] {
  return options.map((option) => ({
    value: option.value,
    label: option.label,
    icon: option.icon,
    color: option.color,
  }))
}

export function PriorityPicker({
  options,
  mobileTitle = `Priority`,
  ...props
}: PriorityPickerProps) {
  const items = priorityPickerItems(options)
  const shared = { items, mobileTitle }
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return <Picker mode="multi" {...shared} {...rest} />
  }
  const { mode: _mode, ...rest } = props
  return <Picker mode="single" {...shared} {...rest} />
}
