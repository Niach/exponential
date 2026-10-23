import { createFileRoute } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { AppSettingsPage } from "@/components/apps/app-settings-page"
import { SettingsSectionGuard, useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/apps/new`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.newApp, `Settings`) }] }),
  component: SettingsNewApp,
})

// VAPP-110: "New app" = the app settings page in CREATE mode; Create opens
// the editor. Any member.
function SettingsNewApp() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && <AppSettingsPage team={team} app={null} canManage />}
    </SettingsSectionGuard>
  )
}
