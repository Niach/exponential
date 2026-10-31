import { useEffect, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { APPS_COPY, APPS_EDITOR_TABS } from "@exp/domain-contract/apps"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { AppEditor } from "@/components/apps/app-editor"
import { useApp, useApps } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"

// VAPP-110: one app's EDITOR (fixtures/apps.json): chat beside the live
// preview, versions. `$appSlug` = the app's slug in the team. VAPP-176: a
// draft's slug follows its new name, so the page keys the app by ID and
// replaces the route in place (the editor never remounts: that would end
// the editor session mid-turn). VAPP-177: `?tab=<id>` opens that tab (phones:
// versions/settings = the menu's sheet over Chat; `editorTab`).

const NavAppsIcon = conceptIcon(`nav-apps`)
const SYNC_GRACE_MS = 4_000

type AppEditorSearch = { tab?: string }

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug`)({
  validateSearch: (search: Record<string, unknown>): AppEditorSearch => {
    const tab = search[APPS_EDITOR_TABS.search]
    return typeof tab === `string` && tab ? { tab } : {}
  },
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
  const { tab: requestedTab } = Route.useSearch()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const navigate = useNavigate()
  const { app: bySlug, isReady } = useApp(team?.id, appSlug)
  const { apps } = useApps(team?.id)
  const lastId = useRef<string | null>(null)
  const app = bySlug ?? (lastId.current ? (apps.find((row) => row.id === lastId.current) ?? null) : null)
  if (app) lastId.current = app.id
  useEffect(() => {
    if (app && app.slug !== appSlug) {
      void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug }, replace: true })
    }
  }, [app, appSlug, teamSlug, navigate])
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
      canManage={permissions.isOwner || app.createdBy === (session?.user?.id ?? null)}
      canGrant={permissions.isOwner}
      requestedTab={requestedTab ?? null}
    />
  )
}
