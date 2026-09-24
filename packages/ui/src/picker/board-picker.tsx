import type { ReactNode } from "react"

import { BOARD_ICON_OPTIONS } from "../board-icons"
import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the board picker: every board row draws its ICON and
// COLOUR, everywhere (EXP-1019's ask, delivered by EXP-1021 through this
// file). Consumers: the composer dialog, the create-issue dialog, move-to-
// board, the widget/board settings.

export interface BoardPickerBoard {
  id: string
  name: string
  /** Contract `boardIcon`; absent = the default glyph. */
  icon?: string | null
  /** The board's hex colour; absent = the foreground. */
  color?: string | null
}

export interface BoardPickerProps {
  boards: readonly BoardPickerBoard[]
  /** The picked board id, or null. */
  value: string | null
  onChange: (boardId: string) => void
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

/** A board row → a picker item (icon + colour). */
export function boardPickerItems(boards: readonly BoardPickerBoard[]): PickerItem[] {
  return boards.map((board) => ({
    value: board.id,
    label: board.name,
    icon: BOARD_ICON_OPTIONS.find((option) => option.name === board.icon)?.icon,
    color: board.color ?? undefined,
  }))
}

export function BoardPicker({
  boards,
  emptyText = `No boards`,
  mobileTitle = `Board`,
  ...props
}: BoardPickerProps) {
  return (
    <Picker
      mode="single"
      items={boardPickerItems(boards)}
      emptyText={emptyText}
      mobileTitle={mobileTitle}
      {...props}
    />
  )
}
