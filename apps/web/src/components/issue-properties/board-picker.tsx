import { useMemo, useState } from "react"
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
import { Button } from "@/components/ui/button"
import type { Board } from "@/db/schema"

interface BoardPickerProps {
  disabled?: boolean
  teamId: string
  selectedBoardId: string
  onSelect: (boardId: string) => void | Promise<void>
}

// Move-to-board picker for the issue detail view (EXP-57): single-select
// over the team's boards (same team only; trashed boards never
// reach the client). MobilePopover + Command
// structure; picking the current board is a no-op. The server renumbers the
// issue in the target board (EXP-42 → ABC-17).
export function BoardPicker({
  disabled,
  teamId,
  selectedBoardId,
  onSelect,
}: BoardPickerProps) {
  const [open, setOpen] = useState(false)

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

  const handlePick = (boardId: string) => {
    setOpen(false)
    if (boardId !== selectedBoardId) {
      void onSelect(boardId)
    }
  }

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(o) => {
        if (disabled) return
        setOpen(o)
      }}
    >
      <MobilePopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          disabled={disabled}
        >
          <div
            className="h-2.5 w-2.5 shrink-0 rounded-full"
            style={{ backgroundColor: selectedBoard?.color ?? `#71717a` }}
          />
          {selectedBoard ? (
            <span className="max-w-[7.5rem] truncate">
              {selectedBoard.name}
            </span>
          ) : (
            `Board`
          )}
        </Button>
      </MobilePopoverTrigger>
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
                  onSelect={() => handlePick(board.id)}
                  className="flex items-center gap-2"
                >
                  <div
                    className="h-2.5 w-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: board.color }}
                  />
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
  )
}
