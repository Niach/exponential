import { useMemo, useState, type ReactNode } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Check } from "lucide-react"
import { boardCollection } from "@/lib/collections"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Pill } from "@/components/ui/pill"
import { MoveBoardConfirmDialog } from "@/components/issue-properties/move-board-confirm"
import { BoardGlyph } from "@/components/board-glyph"
import type { Board } from "@/db/schema"

interface BoardPickerProps {
  disabled?: boolean
  teamId: string
  selectedBoardId: string
  // Named in the confirmation copy; absent reads as "this issue".
  issueIdentifier?: string | null
  onSelect: (boardId: string) => void | Promise<void>
  // Controlled mode (EXP-687): the mobile issue-detail `…` menu opens the
  // picker from a menu item, so it owns the open state and renders no trigger
  // of its own. The MoveBoardConfirmDialog stays here either way.
  open?: boolean
  onOpenChange?: (open: boolean) => void
  hideTrigger?: boolean
  // Replaces the default chip (the mobile properties sheet renders the picker
  // as a full-width property row) — same contract as `AssigneePicker`.
  trigger?: ReactNode
}

// Move-to-board picker for the issue detail view (EXP-57): single-select
// over the team's boards (same team only; trashed boards never
// reach the client). MobilePopover + Command
// structure; picking the current board is a no-op. The server renumbers the
// issue in the target board (EXP-42 → ABC-17) — which is why the pick lands in
// the shared MoveBoardConfirmDialog first (EXP-426).
export function BoardPicker({
  disabled,
  teamId,
  selectedBoardId,
  issueIdentifier,
  onSelect,
  open: controlledOpen,
  onOpenChange,
  hideTrigger,
  trigger,
}: BoardPickerProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false)
  const open = controlledOpen ?? uncontrolledOpen
  const setOpen = onOpenChange ?? setUncontrolledOpen
  const [pendingBoard, setPendingBoard] = useState<Board | null>(null)

  const { data: boardRows } = useLiveQuery(
    (q) =>
      teamId
        ? q
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, teamId))
        : undefined,
    [teamId]
  )

  const boards = useMemo(
    () =>
      [...((boardRows ?? []) as Board[])].sort((left, right) =>
        left.name.localeCompare(right.name)
      ),
    [boardRows]
  )
  const selectedBoard =
    boards.find((board) => board.id === selectedBoardId) ?? null

  const handlePick = (board: Board) => {
    setOpen(false)
    if (board.id !== selectedBoardId) {
      setPendingBoard(board)
    }
  }

  return (
    <>
      <MobilePopover
        open={disabled ? false : open}
        onOpenChange={(o) => {
          if (disabled) return
          setOpen(o)
        }}
      >
        {!hideTrigger && (
          <MobilePopoverTrigger asChild>
            {trigger ?? (
              <Pill mode="action" disabled={disabled}>
                <BoardGlyph
                  board={selectedBoard ?? { color: `#71717a` }}
                  className="size-3.5"
                />
                {selectedBoard ? (
                  <span className="max-w-[7.5rem] truncate">
                    {selectedBoard.name}
                  </span>
                ) : (
                  `Board`
                )}
              </Pill>
            )}
          </MobilePopoverTrigger>
        )}
        <MobilePopoverContent
          className="w-[14rem] p-0"
          align="start"
          mobileTitle="Move to board"
        >
          <Command>
            <CommandInput placeholder="Move to board..." />
            <CommandList>
              <CommandEmpty>No boards found.</CommandEmpty>
              <CommandGroup>
                {boards.map((board) => (
                  <CommandItem
                    key={board.id}
                    // Name keeps cmdk text filtering working; the id suffix
                    // keeps values unique when two boards share a name.
                    value={`${board.name} ${board.id}`}
                    onSelect={() => handlePick(board)}
                    className="flex items-center gap-2"
                  >
                    <BoardGlyph board={board} className="size-3.5" />
                    <span className="min-w-0 truncate text-sm">
                      {board.name}
                    </span>
                    {board.id === selectedBoardId && (
                      <Check className="ml-auto size-3.5 shrink-0" />
                    )}
                  </CommandItem>
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        </MobilePopoverContent>
      </MobilePopover>

      <MoveBoardConfirmDialog
        board={pendingBoard}
        issueIdentifier={issueIdentifier}
        onCancel={() => setPendingBoard(null)}
        onConfirm={(board) => void onSelect(board.id)}
      />
    </>
  )
}
