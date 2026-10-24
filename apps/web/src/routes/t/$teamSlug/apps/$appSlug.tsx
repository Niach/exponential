import { useEffect, useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { AppEditor } from "@/components/apps/app-editor"
import { useApp } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"

// VAPP-110: one app's EDITOR (fixtures/apps.json): chat beside the live
// preview, versions. `$appSlug` = the app's slug in the team.

const NavAppsIcon = conceptIcon(`nav-apps`)
const SYNC_GRACE_MS = 4_000

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: AppEditorPage,
})

function AppEditorPage() {
  const { teamSlug, appSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const { app, isReady } = useApp(team?.id, appSlug)
  usePageTitle(app ? pageTitle(app.name, APPS_COPY.title) : undefined)

  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [appSlug])

  if (!team || !app) {
    if (team && isReady && graceOver) {
      return (
        <div className="flex h-full flex-col items-center justify-center">
          <EmptyState icon={NavAppsIcon} title="Not found" description="It may still be syncing, or it was deleted." size="page" />
        </div>
      )
    }
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }

  return (
    <AppEditor
      key={app.id}
      app={app}
      teamId={team.id}
      teamSlug={teamSlug}
      me={session?.user?.id ?? null}
      isMember={permissions.isMember}
    />
  )
}
