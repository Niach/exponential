import type { LucideIcon } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { boardIconValues } from "@exp/db-schema/domain"
import { PICKABLE_ICONS, SEMANTIC_ICONS } from "@exp/icons"
import { ICON_COMPONENTS } from "@/lib/icons.generated"

// EXP-273: the curated set is no longer a per-client hand-map. `boardIcon`
// values ARE Lucide names (the domain contract and @exp/icons `pickable` are
// byte-equal, locked by icons.test.ts), so resolving one is a lookup in the
// generated registry — and iOS/Android/desktop resolve the same name to the
// same art instead of each substituting its own native glyph.
export const BOARD_ICON_COMPONENTS: Record<BoardIcon, LucideIcon> =
  Object.fromEntries(
    boardIconValues.map((name) => [name, ICON_COMPONENTS[name]])
  ) as Record<BoardIcon, LucideIcon>

export const BOARD_ICON_OPTIONS = PICKABLE_ICONS.map((name) => ({
  name: name as BoardIcon,
  icon: ICON_COMPONENTS[name],
}))

// Resolve a board's canonical icon NAME: the stored `icon` when set (the
// drop-type migration backfilled every row, so the fallback is a cosmetic
// safety net derived from repo presence). Feeds both the display component
// below and the icon picker's selected value.
export function getBoardIconName(board: {
  icon?: string | null
  repositoryId?: string | null
}): BoardIcon {
  if (board.icon && board.icon in BOARD_ICON_COMPONENTS) {
    return board.icon as BoardIcon
  }
  return board.repositoryId ? `code` : `square-kanban`
}

// Resolve a board's display icon component.
export function getBoardIcon(board: {
  icon?: string | null
  repositoryId?: string | null
}): LucideIcon {
  return BOARD_ICON_COMPONENTS[getBoardIconName(board)]
}

// EXP-273: an action's display glyph — the same curated set as boards, so the
// two pickers and both fallbacks live together. Actions authored before the
// column existed (and the builtins' own rows, which set it explicitly) fall
// back to the registry's generic action glyph.
export function getActionIcon(action: {
  icon?: string | null
}): LucideIcon {
  if (action.icon && action.icon in BOARD_ICON_COMPONENTS) {
    return BOARD_ICON_COMPONENTS[action.icon as BoardIcon]
  }
  return ICON_COMPONENTS[SEMANTIC_ICONS[`action-default`]]
}
