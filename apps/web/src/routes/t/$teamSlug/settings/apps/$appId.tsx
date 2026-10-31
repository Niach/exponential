import { useEffect, useMemo } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { SkeletonRows } from "@exp/ui"
import { useApps } from "@/hooks/use-apps"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/apps/$appId`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title, `Settings`) }] }),
  component: SettingsAppRedirect,
})

// VAPP-110: the old one-app settings page. VAPP-177: it REDIRECTS (replace)
// to the app's editor on its Settings tab, the slug resolved off the synced
// row (the route id IS the app id); a stale id lands on the index.
function SettingsAppRedirect() {
  const { teamSlug, appId } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const { apps, isReady } = useApps(team?.id)
  const app = useMemo(() => apps.find((entry) => entry.id === appId) ?? null, [apps, appId])
  const navigate = useNavigate()

  useEffect(() => {
    if (!isReady || !team) return
    if (app) {
      void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug }, search: { tab: `settings` }, replace: true })
      return
    }
    void navigate({ to: `/t/$teamSlug/settings/apps`, params: { teamSlug }, replace: true })
  }, [app, isReady, navigate, team, teamSlug])

  return (
    <div className="p-6">
      <SkeletonRows />
    </div>
  )
}
