import { createFileRoute } from "@tanstack/react-router"
import { TeamLabelsSection } from "@/components/team/labels-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/labels`)({
  head: () => ({
    meta: [{ title: pageTitle(`Labels`, `Settings`) }],
  }),
  component: SettingsLabels,
})

function SettingsLabels() {
  const { teamSlug } = Route.useParams()
  const { team } = useSettingsPage(teamSlug)

  if (!team) return null
  return <TeamLabelsSection teamId={team.id} />
}
