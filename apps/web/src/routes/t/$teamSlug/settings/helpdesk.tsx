import { createFileRoute } from "@tanstack/react-router"
import { TeamHelpdeskSection } from "@/components/team/helpdesk-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/helpdesk`)({
  component: SettingsHelpdesk,
})

function SettingsHelpdesk() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageWidgets}
    >
      {team && <TeamHelpdeskSection team={team} />}
    </SettingsSectionGuard>
  )
}
