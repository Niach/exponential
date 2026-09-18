import { useMemo, useState, type ReactNode } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { boardCollection } from "@/lib/collections"
import { Combobox, Pill, type PickerOption, BoardGlyph } from "@exp/ui"
import { MoveBoardConfirmDialog } from "@/components/issue-properties/move-board-confirm"
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

// Move-to-board picker for the issue detail view (EXP-57): single-select over
// the team's boards (same team only; trashed boards never reach the client) on
// the shared `Combobox` (EXP-941); picking the current board is a no-op. The
// server renumbers the issue in the target board (EXP-42 → ABC-17) — which is
// why the pick lands in the shared MoveBoardConfirmDialog first (EXP-426), so
// the LIST never calls `onSelect`, it only stages a pending board.
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
  const boardsById = useMemo(
    () => new Map(boards.map((board) => [board.id, board])),
    [boards]
  )
  const options = useMemo<PickerOption[]>(
    () => boards.map((board) => ({ value: board.id, label: board.name })),
    [boards]
  )
  const selectedBoard = boardsById.get(selectedBoardId) ?? null

  const handlePick = (boardId: string | null) => {
    const board = boardId ? boardsById.get(boardId) : undefined
    if (board && board.id !== selectedBoardId) {
      setPendingBoard(board)
    }
  }

  return (
    <>
      <Combobox
        options={options}
        value={selectedBoardId}
        onChange={handlePick}
        disabled={disabled}
        open={disabled ? false : open}
        onOpenChange={setOpen}
        hideTrigger={hideTrigger}
        width="sm"
        mobileTitle="Move to board"
        placeholder="Move to board..."
        emptyText="No boards found."
        renderOption={(option) => {
          const board = boardsById.get(option.value)
          return (
            <>
              {board && <BoardGlyph board={board} className="size-3.5" />}
              <span className="min-w-0 flex-1 truncate text-sm">
                {option.label}
              </span>
            </>
          )
        }}
        renderTrigger={() =>
          trigger ?? (
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
          )
        }
      />

      <MoveBoardConfirmDialog
        board={pendingBoard}
        issueIdentifier={issueIdentifier}
        onCancel={() => setPendingBoard(null)}
        onConfirm={(board) => void onSelect(board.id)}
      />
    </>
  )
}
