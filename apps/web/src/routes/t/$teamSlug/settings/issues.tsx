import { createFileRoute } from "@tanstack/react-router"
import { TeamIssuesSection } from "@/components/team/issues-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// EXP-630: the Issues settings page (estimate scale + PR automation);
// Labels and Statuses hang under it in the nav. Member-visible like them —
// the PR automation pickers are member-managed, the scale owner-only.
export const Route = createFileRoute(`/t/$teamSlug/settings/issues`)({
  head: () => ({
    meta: [{ title: pageTitle(`Issues`, `Settings`) }],
  }),
  component: SettingsIssues,
})

function SettingsIssues() {
  const { teamSlug } = Route.useParams()
  const { team, permissions } = useSettingsPage(teamSlug)

  if (!team) return null
  return <TeamIssuesSection team={team} canEdit={permissions.canManageTeam} />
}
