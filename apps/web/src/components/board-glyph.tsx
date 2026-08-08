import { cn } from "@/lib/utils"
import { getBoardIcon } from "@/lib/board-icons"

// EXP-449: the one board glyph — every board picker, breadcrumb and caption
// renders the board's icon tinted with its color (the anonymous color dot is
// gone everywhere, matching the sidebar and the native switcher sheets).
export function BoardGlyph({
  board,
  className,
}: {
  board: { icon?: string | null; repositoryId?: string | null; color?: string | null }
  className?: string
}) {
  const Icon = getBoardIcon(board)
  return (
    <Icon
      className={cn(`size-4 shrink-0`, className)}
      style={board.color ? { color: board.color } : undefined}
    />
  )
}
