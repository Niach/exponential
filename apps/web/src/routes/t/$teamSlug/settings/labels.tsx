import { createFileRoute } from "@tanstack/react-router"
import { TeamLabelsSection } from "@/components/team/labels-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/labels`)({
  component: SettingsLabels,
})

function SettingsLabels() {
  const { teamSlug } = Route.useParams()
  const { team } = useSettingsPage(teamSlug)

  if (!team) return null
  return <TeamLabelsSection teamId={team.id} />
}
