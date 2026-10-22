import { useEffect, useMemo } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { AppSettingsPage } from "@/components/apps/app-settings-page"
import { useApps } from "@/hooks/use-apps"
import { SettingsSectionGuard, useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle, usePageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/apps/$appId`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title, `Settings`) }] }),
  component: SettingsApp,
})

// VAPP-110: one app's settings page off the live synced row (the route id
// IS the app id); a stale id lands on the index.
function SettingsApp() {
  const { teamSlug, appId } = Route.useParams()
  const { session, team, permissions, resolved } = useSettingsPage(teamSlug)
  const { apps, isReady } = useApps(team?.id)
  const app = useMemo(() => apps.find((entry) => entry.id === appId && !entry.builtin) ?? null, [apps, appId])
  usePageTitle(app ? pageTitle(app.name, `Settings`) : undefined)
  const navigate = useNavigate()

  useEffect(() => {
    if (!isReady || !team || app) return
    void navigate({ to: `/t/$teamSlug/settings/apps`, params: { teamSlug }, replace: true })
  }, [app, isReady, navigate, team, teamSlug])

  const me = session?.user?.id ?? null
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && app && (
        <AppSettingsPage key={app.id} team={team} app={app} canManage={permissions.isOwner || app.createdBy === me} />
      )}
    </SettingsSectionGuard>
  )
}
