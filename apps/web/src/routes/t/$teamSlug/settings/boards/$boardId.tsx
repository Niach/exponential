import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useTeamBoardsWithReady } from "@/hooks/use-team-data"
import { BoardSettingsPage } from "@/components/team/boards-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/boards/$boardId`)({
  component: SettingsBoard,
})

// EXP-862: one board's settings PAGE (the desktop IDE's EXP-288 pane). Fed
// the LIVE Electric row, so every write reflects via sync and a board
// trashed out from under the page simply stops rendering.
function SettingsBoard() {
  const { teamSlug, boardId } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const board = boards.find((row) => row.id === boardId) ?? null
  const navigate = useNavigate()

  // A board id that resolves to nothing once the snapshot is in (a stale
  // link, a board trashed meanwhile) lands on the index, which picks the
  // first board or the create prompt; never a blank page.
  useEffect(() => {
    if (!boardsReady || board) return
    void navigate({
      to: `/t/$teamSlug/settings/boards`,
      params: { teamSlug },
      replace: true,
    })
  }, [board, boardsReady, navigate, teamSlug])

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && board && <BoardSettingsPage board={board} team={team} />}
    </SettingsSectionGuard>
  )
}
