import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Check, FolderKanban, Plus } from "lucide-react"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { getBoardIcon } from "@/lib/board-icons"
import { useSession } from "@/hooks/use-session"
import {
  useShowTeamChrome,
  useTeamMemberships,
} from "@/hooks/use-team-data"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"

interface BoardSwitcherSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  activeBoardSlug?: string
}

const rowClass = `flex h-11 w-full items-center gap-3 rounded-md px-3 text-left text-sm hover:bg-muted/50`

// Mobile board/team switcher (EXP-189), mirroring the native apps'
// BoardSwitcherSheet: a bottom sheet listing the team's boards and — for
// multi-team users (same solo-mode gate as the sidebar switcher) — the
// user's other teams.
export function BoardSwitcherSheet({
  open,
  onOpenChange,
  teamSlug,
  team,
  boards,
  activeBoardSlug,
}: BoardSwitcherSheetProps) {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const isAuthed = Boolean(session?.user)
  const { myTeams } = useTeamMemberships(session?.user?.id)
  const showTeams = useShowTeamChrome(team?.id, session?.user?.id)
  const [createBoardOpen, setCreateBoardOpen] = useState(false)
  const [createTeamOpen, setCreateTeamOpen] = useState(false)

  return (
    <>
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent
          side="bottom"
          className="max-h-[75dvh] gap-0 overflow-y-auto rounded-t-xl pb-[max(1rem,env(safe-area-inset-bottom))]"
        >
          <SheetHeader className="pb-2">
            <SheetTitle>Boards</SheetTitle>
          </SheetHeader>
          <div className="flex flex-col px-2">
            {!boards || boards.length === 0 ? (
              <div className="flex h-11 items-center gap-3 px-3 text-sm text-muted-foreground">
                <FolderKanban className="size-4 shrink-0" />
                No boards yet
              </div>
            ) : (
              boards.map((board) => {
                const TypeIcon = getBoardIcon(board)
                const isActive = board.slug === activeBoardSlug
                return (
                  <button
                    key={board.id}
                    type="button"
                    className={cn(rowClass, isActive && `bg-muted/50`)}
                    onClick={() => {
                      onOpenChange(false)
                      navigate({
                        to: `/t/$teamSlug/boards/$boardSlug`,
                        params: { teamSlug, boardSlug: board.slug },
                      })
                    }}
                  >
                    <TypeIcon
                      className="size-4 shrink-0"
                      style={{ color: board.color }}
                    />
                    <span className="min-w-0 flex-1 truncate">
                      {board.name}
                    </span>
                    {isActive && (
                      <Check className="size-4 shrink-0 text-muted-foreground" />
                    )}
                  </button>
                )
              })
            )}
            {isAuthed && team && (
              <button
                type="button"
                className={cn(rowClass, `text-muted-foreground`)}
                onClick={() => setCreateBoardOpen(true)}
              >
                <Plus className="size-4 shrink-0" />
                Create board
              </button>
            )}
          </div>
          {isAuthed && showTeams && (
            <>
              <div className="px-4 pb-2 pt-4 text-sm font-semibold">
                Teams
              </div>
              <div className="flex flex-col px-2">
                {myTeams.map((ws) => (
                  <button
                    key={ws.id}
                    type="button"
                    className={cn(
                      rowClass,
                      ws.slug === teamSlug && `bg-muted/50`
                    )}
                    onClick={() => {
                      onOpenChange(false)
                      navigate({
                        to: `/t/$teamSlug`,
                        params: { teamSlug: ws.slug },
                      })
                    }}
                  >
                    <div className="flex size-5 shrink-0 items-center justify-center rounded bg-primary text-[0.625rem] font-bold text-primary-foreground">
                      {ws.name[0]?.toUpperCase()}
                    </div>
                    <span className="min-w-0 flex-1 truncate">
                      {ws.name}
                    </span>
                    {ws.slug === teamSlug && (
                      <Check className="size-4 shrink-0 text-muted-foreground" />
                    )}
                  </button>
                ))}
                <button
                  type="button"
                  className={cn(rowClass, `text-muted-foreground`)}
                  onClick={() => setCreateTeamOpen(true)}
                >
                  <Plus className="size-4 shrink-0" />
                  New team
                </button>
              </div>
            </>
          )}
        </SheetContent>
      </Sheet>
      {team && isAuthed && (
        <CreateBoardDialog
          open={createBoardOpen}
          onOpenChange={setCreateBoardOpen}
          team={team}
        />
      )}
      {isAuthed && (
        <CreateTeamDialog
          open={createTeamOpen}
          onOpenChange={setCreateTeamOpen}
        />
      )}
    </>
  )
}
