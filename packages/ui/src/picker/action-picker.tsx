import type { ReactNode } from "react"

import { BOARD_ICON_OPTIONS } from "../board-icons"
import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the action picker: the team's actions (and the two
// listed builtins) by curated icon + name. The composer's action chip and
// the automation editor pick one.

export interface ActionPickerAction {
  id: string
  name: string
  /** Contract `boardIcon` (the curated action set); absent = the default. */
  icon?: string | null
  description?: ReactNode
}

export interface ActionPickerProps {
  actions: readonly ActionPickerAction[]
  value: string | null
  onChange: (actionId: string) => void
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export function actionPickerItems(actions: readonly ActionPickerAction[]): PickerItem[] {
  return actions.map((action) => ({
    value: action.id,
    label: action.name,
    icon: BOARD_ICON_OPTIONS.find((option) => option.name === action.icon)?.icon,
    description: action.description,
  }))
}

export function ActionPicker({
  actions,
  emptyText = `No actions`,
  mobileTitle = `Actions`,
  search = true,
  ...props
}: ActionPickerProps) {
  return (
    <Picker
      mode="single"
      items={actionPickerItems(actions)}
      emptyText={emptyText}
      mobileTitle={mobileTitle}
      search={search}
      {...props}
    />
  )
}
