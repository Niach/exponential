import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useTeamBoardsWithReady } from "@/hooks/use-team-data"
import { BoardsEmptyState } from "@/components/team/boards-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/boards/`)({
  component: SettingsBoardsIndex,
})

// EXP-862: the Boards section has no list page any more — the settings nav
// IS the list. Bare /settings/boards forwards to the first board; a team
// with none renders the create prompt instead.
function SettingsBoardsIndex() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const first = boards[0]

  useEffect(() => {
    if (!first) return
    void navigate({
      to: `/t/$teamSlug/settings/boards/$boardId`,
      params: { teamSlug, boardId: first.id },
      replace: true,
    })
  }, [first, navigate, teamSlug])

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && boardsReady && !first && <BoardsEmptyState team={team} />}
    </SettingsSectionGuard>
  )
}
