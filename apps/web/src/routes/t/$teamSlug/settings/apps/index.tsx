import { createFileRoute } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { AppsList } from "@/components/apps/apps-list"
import { useApps } from "@/hooks/use-apps"
import { SettingsSectionGuard, useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/apps/`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title, `Settings`) }] }),
  component: SettingsAppsIndex,
})

// VAPP-110: Settings → Apps, the Agents page's twin: the Apps band (one row
// per app, New app) whose rows open the app's settings page.
function SettingsAppsIndex() {
  const { teamSlug } = Route.useParams()
  const { session, team, permissions, resolved } = useSettingsPage(teamSlug)
  const { apps, isReady } = useApps(team?.id)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && isReady && (
        <AppsList
          teamId={team.id}
          teamSlug={teamSlug}
          apps={apps}
          me={session?.user?.id ?? null}
          isOwner={permissions.isOwner}
          rowsOpen="settings"
        />
      )}
    </SettingsSectionGuard>
  )
}
