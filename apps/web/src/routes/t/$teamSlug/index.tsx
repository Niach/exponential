import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { FolderKanban } from "lucide-react"
import {
  useTeamBySlug,
  useTeamBoardsWithReady,
} from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { EmptyState } from "@/components/empty-state"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { Button } from "@/components/ui/button"
import { readLastVisited } from "@/lib/last-visited"

export const Route = createFileRoute(`/t/$teamSlug/`)({
  component: TeamIndexPage,
})

function TeamIndexPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const permissions = useTeamPermissions(team)
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    if (boards && boards.length > 0) {
      // EXP-69: prefer this device's last-used board when it still exists
      // in the team; a stale slug (board deleted/trashed) degrades to
      // the first board, and that board visit rewrites the stored entry.
      const last = readLastVisited()
      const preferred =
        last?.teamSlug === teamSlug && last.boardSlug
          ? boards.find((board) => board.slug === last.boardSlug)
          : undefined
      navigate({
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: {
          teamSlug,
          boardSlug: (preferred ?? boards[0]).slug,
        },
        replace: true,
      })
    }
  }, [boards, teamSlug, navigate])

  // Until the boards snapshot lands, an empty list means "still syncing",
  // not "no boards" — showing the create-your-first-board state early
  // flashes it at every member of a board-having team (REV2-59 class).
  if (!boardsReady || boards.length > 0) {
    return null
  }

  // EXP-698 r5: the empty TEAM carries the checklist the empty BOARD does —
  // same wording as the IDE and the natives, cards inline underneath. The
  // whole thing is one scroll column: on a phone the cards are taller than
  // the viewport.
  return (
    <div className={`h-full overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
      <div className="flex min-h-[60vh] flex-1 items-center justify-center">
        <EmptyState
          icon={FolderKanban}
          title="No boards yet"
          description="Create a board to start tracking work."
        >
          <Button onClick={() => setCreateOpen(true)}>
            <FolderKanban className="mr-2 size-4" />
            Create a board
          </Button>
        </EmptyState>
      </div>
      {/* Members only — the guidance block is meaningless before the viewer's
          own member row has synced (board parity). */}
      {team && permissions.isMember && (
        <GettingStartedSection team={team} teamSlug={teamSlug} />
      )}
      {team && (
        <CreateBoardDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          team={team}
        />
      )}
    </div>
  )
}
