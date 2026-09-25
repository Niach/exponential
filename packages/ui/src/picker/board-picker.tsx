import type { ReactNode } from "react"

import { getBoardIcon } from "../board-icons"
import {
  Picker,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-1029 contract — the board picker: every board row draws its ICON and
// COLOUR, everywhere (EXP-1019's ask, delivered by EXP-1021 through this
// file). Consumers: the composer dialog, the create-issue dialog, move-to-
// board, the widget/board settings.

export interface BoardPickerBoard {
  id: string
  name: string
  /** Contract `boardIcon`; absent = the repo/default glyph. */
  icon?: string | null
  /** The board's hex colour; absent = the foreground. */
  color?: string | null
  /** A repo-backed board with no icon of its own draws the repo glyph
   *  (EXP-449: the anonymous colour dot is gone everywhere). */
  repositoryId?: string | null
}

interface BoardPickerBase extends PickerSurfaceProps {
  boards: readonly BoardPickerBoard[]
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

/** Single by default (move-to-board, the create dialog's board chip); `multi`
 *  is the FILTER arm — an automation trigger scoped to a set of boards. */
export type BoardPickerProps = BoardPickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (boardId: string) => void }
    | {
        mode: `multi`
        value: readonly string[]
        onChange: (boardIds: string[]) => void
        max?: number
      }
  )

/** A board row → a picker item (icon + colour). */
export function boardPickerItems(boards: readonly BoardPickerBoard[]): PickerItem[] {
  return boards.map((board) => ({
    value: board.id,
    label: board.name,
    icon: getBoardIcon(board),
    color: board.color ?? undefined,
  }))
}

export function BoardPicker({
  boards,
  emptyText = `No boards`,
  mobileTitle = `Board`,
  // On by default: every board picker the web had before EXP-1021 carried a
  // filter field, and a team's board list outgrows a menu quickly.
  search = true,
  ...props
}: BoardPickerProps) {
  const shared = {
    items: boardPickerItems(boards),
    emptyText,
    mobileTitle,
    search,
  }
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return <Picker mode="multi" {...shared} {...rest} />
  }
  const { mode: _mode, ...rest } = props
  return <Picker mode="single" {...shared} {...rest} />
}
