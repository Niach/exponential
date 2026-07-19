import { createFileRoute } from "@tanstack/react-router"
import { TeamWidgetSection } from "@/components/team/widget-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/widget`)({
  component: SettingsWidget,
})

function SettingsWidget() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageWidgets}
    >
      {team && <TeamWidgetSection team={team} />}
    </SettingsSectionGuard>
  )
}
