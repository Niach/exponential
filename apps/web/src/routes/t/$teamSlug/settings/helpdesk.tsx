import { createFileRoute } from "@tanstack/react-router"
import { TeamHelpdeskSection } from "@/components/team/helpdesk-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/helpdesk`)({
  head: () => ({
    meta: [{ title: pageTitle(`Helpdesk`, `Settings`) }],
  }),
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
