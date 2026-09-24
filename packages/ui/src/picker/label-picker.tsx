import type { ReactNode } from "react"

import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
// its colour dot + name; the surface stays open across toggles. The issue
// properties, the create-issue dialog, the bulk edit and the board filter.

export interface LabelPickerLabel {
  id: string
  name: string
  /** The label's hex. */
  color?: string | null
}

export interface LabelPickerProps {
  labels: readonly LabelPickerLabel[]
  value: readonly string[]
  onChange: (labelIds: string[]) => void
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  /** On by default: a team's label list outgrows a menu quickly. */
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export function labelPickerItems(labels: readonly LabelPickerLabel[]): PickerItem[] {
  return labels.map((label) => ({
    value: label.id,
    label: label.name,
    color: label.color ?? undefined,
  }))
}

export function LabelPicker({
  labels,
  search = true,
  emptyText = `No labels`,
  mobileTitle = `Labels`,
  ...props
}: LabelPickerProps) {
  return (
    <Picker
      mode="multi"
      items={labelPickerItems(labels)}
      search={search}
      emptyText={emptyText}
      mobileTitle={mobileTitle}
      {...props}
    />
  )
}
