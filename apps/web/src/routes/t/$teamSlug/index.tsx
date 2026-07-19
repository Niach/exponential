import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { FolderKanban } from "lucide-react"
import {
  useTeamBySlug,
  useTeamBoards,
} from "@/hooks/use-team-data"
import { EmptyState } from "@/components/empty-state"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { Button } from "@/components/ui/button"
import { readLastVisited } from "@/lib/last-visited"

export const Route = createFileRoute(`/t/$teamSlug/`)({
  component: TeamIndexPage,
})

function TeamIndexPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
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

  if (!boards || boards.length > 0) {
    return null
  }

  return (
    <div className="flex h-full flex-1 items-center justify-center">
      <EmptyState
        icon={FolderKanban}
        title="Create your first board"
        description="Boards hold your issues. Create one to start tracking work."
      >
        <Button onClick={() => setCreateOpen(true)}>
          <FolderKanban className="mr-2 size-4" />
          Create a board
        </Button>
      </EmptyState>
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
