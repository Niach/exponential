import {
  BookOpen,
  Bug,
  Code2,
  Globe,
  Heart,
  Lightbulb,
  Megaphone,
  MessageCircle,
  Package,
  Rocket,
  Shield,
  SquareKanban,
  Star,
  Terminal,
  Wrench,
  Zap,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { boardIconValues } from "@exp/db-schema/domain"

// Curated icon set (boardIconValues in the domain contract) → lucide
// components, in the picker's display order.
export const BOARD_ICON_COMPONENTS: Record<BoardIcon, LucideIcon> = {
  code: Code2,
  "square-kanban": SquareKanban,
  megaphone: Megaphone,
  bug: Bug,
  rocket: Rocket,
  "book-open": BookOpen,
  globe: Globe,
  heart: Heart,
  star: Star,
  zap: Zap,
  wrench: Wrench,
  shield: Shield,
  package: Package,
  terminal: Terminal,
  lightbulb: Lightbulb,
  "message-circle": MessageCircle,
}

export const BOARD_ICON_OPTIONS = boardIconValues.map((name) => ({
  name,
  icon: BOARD_ICON_COMPONENTS[name],
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
