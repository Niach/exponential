import type { ReactNode } from "react"

import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the status picker: the team's `issue_statuses` rows
// (EXP-314) in `displayOrder`, each by its glyph in its colour (builtins by
// token, customs by `colorHex`). The issue header chip, the create-issue
// dialog, the bulk edit and the board filter pick one (the filter several).
// Resolution of glyph + colour stays in the app's `lib/team-statuses.ts` /
// `lib/status-icons.ts`; the picker takes the resolved row.

export interface StatusPickerStatus {
  id: string
  name: string
  /** Contract `issueStatusCategory`. */
  category: string
  /** The resolved row colour (custom rows' `colorHex`, builtins' token). */
  colorHex?: string | null
  /** The resolved glyph for the row (`lib/status-icons.ts`). */
  icon?: PickerItem[`icon`]
}

interface StatusPickerBase {
  statuses: readonly StatusPickerStatus[]
  trigger: ReactNode
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export type StatusPickerProps = StatusPickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (statusId: string) => void }
    | { mode: `multi`; value: readonly string[]; onChange: (statusIds: string[]) => void }
  )

export function statusPickerItems(statuses: readonly StatusPickerStatus[]): PickerItem[] {
  return statuses.map((status) => ({
    value: status.id,
    label: status.name,
    icon: status.icon,
    color: status.colorHex ?? undefined,
    keywords: [status.name, status.category],
  }))
}

export function StatusPicker({ statuses, emptyText = `No statuses`, ...props }: StatusPickerProps) {
  const items = statusPickerItems(statuses)
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return <Picker mode="multi" items={items} emptyText={emptyText} {...rest} />
  }
  const { mode: _mode, ...rest } = props
  return <Picker mode="single" items={items} emptyText={emptyText} {...rest} />
}
