import { createFileRoute } from "@tanstack/react-router"
import { TeamStatusesSection } from "@/components/team/statuses-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/statuses`)({
  head: () => ({
    meta: [{ title: pageTitle(`Statuses`, `Settings`) }],
  }),
  component: SettingsStatuses,
})

function SettingsStatuses() {
  const { teamSlug } = Route.useParams()
  const { team } = useSettingsPage(teamSlug)

  if (!team) return null
  return <TeamStatusesSection teamId={team.id} />
}
