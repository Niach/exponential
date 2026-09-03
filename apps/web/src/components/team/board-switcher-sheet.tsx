import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import { Plus } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { boardCollection } from "@/lib/collections"
import { useSession } from "@/hooks/use-session"
import { compareBoards, useTeamMemberships } from "@/hooks/use-team-data"
import { BoardGlyph } from "@/components/board-glyph"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { TeamAvatar } from "@/components/team/team-avatar"
import { GlassRow } from "@/components/ui/glass-rows"
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

const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
// EXP-687: one glyph for "boards" across every web surface that moves an
// issue or a reader between them (icons.test.ts gates it).
const NavBoardsIcon = conceptIcon(`nav-boards`)

// The plain, muted "add" rows: no card, no stroke — they are the sheet's
// footers, not entries in the list.
const PLAIN_ROW = `flex h-11 w-full items-center gap-3 rounded-md px-3 text-left text-sm text-muted-foreground hover:bg-glass-active/50`

// Mobile board/team switcher (EXP-189), mirroring the native apps'
// BoardSwitcherSheet. EXP-698 r5 rebuilt it to the shared contract: the user's
// TEAMS are the headings and their boards the carded rows underneath, so
// switching team and board is ONE tap instead of two sections and a tick.
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
  const { myTeams } = useTeamMemberships(session?.user?.id)
  // The board shape spans every team the viewer belongs to, so one query
  // feeds every section (`useTeamBoards` is per-team and cannot be called in
  // a loop).
  const { data: boardRows } = useLiveQuery((query) =>
    query.from({ boards: boardCollection })
  )
  // Two pieces of state, not one: `createBoardTeam` says WHICH team the
  // dialog files into and stays set after a close so the dialog can animate
  // out instead of being unmounted mid-transition.
  const [createBoardTeam, setCreateBoardTeam] = useState<Team | null>(null)
  const [createBoardOpen, setCreateBoardOpen] = useState(false)
  const [createTeamOpen, setCreateTeamOpen] = useState(false)

  // The active team leads; the rest keep their membership order.
  const orderedTeams = useMemo(() => {
    const rest = myTeams.filter((row) => row.slug !== teamSlug)
    const active = myTeams.find((row) => row.slug === teamSlug)
    return active ? [active, ...rest] : myTeams
  }, [myTeams, teamSlug])

  const boardsByTeam = useMemo(() => {
    const map = new Map<string, Board[]>()
    for (const row of (boardRows ?? []) as Board[]) {
      const list = map.get(row.teamId) ?? []
      list.push(row)
      map.set(row.teamId, list)
    }
    // The team the sheet was opened from already has its sorted list from the
    // layout — prefer it so the sheet order matches the sidebar exactly.
    if (team && boards) map.set(team.id, boards)
    for (const [id, list] of map) map.set(id, [...list].sort(compareBoards))
    return map
  }, [boardRows, boards, team])

  return (
    <>
      <Sheet open={open} onOpenChange={onOpenChange}>
        {/* The panel itself never scrolls (EXP-687): it is the sheet frame,
            capped at the shared 90dvh, and the list inside it is the one
            scroll region. */}
        <SheetContent
          side="bottom"
          className="gap-0 pb-[max(1rem,env(safe-area-inset-bottom))]"
        >
          <SheetHeader className="pb-2">
            <SheetTitle>Switch board</SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="flex flex-col gap-4 px-2 pb-2">
              {orderedTeams.map((row) => {
                const teamBoards = boardsByTeam.get(row.id) ?? []
                return (
                  <div key={row.id} className="flex flex-col gap-1.5">
                    <div className="flex items-center gap-2 px-3 pt-1">
                      <TeamAvatar name={row.name} size={18} />
                      <span className="min-w-0 truncate text-sm font-medium">
                        {row.name}
                      </span>
                    </div>
                    {teamBoards.length === 0 && (
                      <div className="flex h-11 items-center gap-3 px-3 text-sm text-muted-foreground">
                        <NavBoardsIcon className="size-4 shrink-0" />
                        No boards yet
                      </div>
                    )}
                    {teamBoards.map((board) => {
                      const isActive =
                        row.slug === teamSlug && board.slug === activeBoardSlug
                      return (
                        <GlassRow
                          key={board.id}
                          interactive
                          className={cn(
                            `gap-3`,
                            isActive &&
                              `border-glass-stroke-active bg-glass-active`
                          )}
                          onClick={() => {
                            onOpenChange(false)
                            navigate({
                              to: `/t/$teamSlug/boards/$boardSlug`,
                              params: {
                                teamSlug: row.slug,
                                boardSlug: board.slug,
                              },
                            })
                          }}
                        >
                          <BoardGlyph board={board} />
                          <span className="min-w-0 flex-1 truncate text-sm">
                            {board.name}
                          </span>
                          <span className="shrink-0 font-mono text-xs text-muted-foreground">
                            {board.prefix}
                          </span>
                          <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
                        </GlassRow>
                      )
                    })}
                    <button
                      type="button"
                      className={PLAIN_ROW}
                      onClick={() => {
                        setCreateBoardTeam(row)
                        setCreateBoardOpen(true)
                      }}
                    >
                      <Plus className="size-4 shrink-0" />
                      Create board
                    </button>
                  </div>
                )
              })}
              <button
                type="button"
                className={PLAIN_ROW}
                onClick={() => setCreateTeamOpen(true)}
              >
                <Plus className="size-4 shrink-0" />
                New team
              </button>
            </div>
          </div>
        </SheetContent>
      </Sheet>
      {createBoardTeam && (
        <CreateBoardDialog
          open={createBoardOpen}
          onOpenChange={setCreateBoardOpen}
          team={createBoardTeam}
        />
      )}
      <CreateTeamDialog
        open={createTeamOpen}
        onOpenChange={setCreateTeamOpen}
      />
    </>
  )
}
