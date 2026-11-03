import { useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate, useParams, useSearch } from "@tanstack/react-router"
import { APPS_COPY, APPS_EDITOR_TABS, screenPath } from "@exp/domain-contract/apps"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { AppRunView } from "@/components/apps/app-run-view"
import { useApp, useApps } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { routeScreenParams, type ScreenRequest } from "@/lib/app-screens"

// VAPP-242 (fixtures/apps.json THE RUN VIEW): `/t/$teamSlug/apps/$appSlug`
// RUNS the app full screen (the published package, the screens shell); the
// child route `$appSlug/$screen` (its query = the screen's params, apps.json
// `screens`) is the deep link to one screen, read here so a screen change
// never remounts the host. A draft's slug may follow its new name: the page
// keys the app by ID and replaces the route in place. An old editor link
// (`?tab=<id>`) replaces itself with THE EDITOR on that tab.

const NavAppsIcon = conceptIcon(`nav-apps`)
const SYNC_GRACE_MS = 4_000

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title) }] }),
  beforeLoad: async ({ context, location, params, search }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
    const tab = (search as Record<string, unknown>)[APPS_EDITOR_TABS.search]
    if (typeof tab === `string` && tab) {
      throw redirect({ to: `/t/$teamSlug/app-editor/$appSlug`, params, search: { tab }, replace: true })
    }
  },
  component: AppRunPage,
})

function AppRunPage() {
  const { teamSlug, appSlug } = Route.useParams()
  const { screen } = useParams({ strict: false }) as { screen?: string }
  const search = useSearch({ strict: false }) as Record<string, unknown>
  const paramsKey = screen ? JSON.stringify(routeScreenParams(search)) : ``
  const requestedScreen = useMemo<ScreenRequest | null>(
    () => (screen ? { screen, params: JSON.parse(paramsKey) as Record<string, string> } : null),
    [screen, paramsKey]
  )
  const { data: session } = useSession()
  const me = session?.user?.id ?? null
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
      if (requestedScreen) void navigate({ href: screenPath(teamSlug, app.slug, requestedScreen.screen, requestedScreen.params), replace: true })
      else void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug }, replace: true })
    }
  }, [app, appSlug, teamSlug, navigate, requestedScreen])
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
    <AppRunView
      key={app.id}
      app={app}
      teamId={team.id}
      teamSlug={teamSlug}
      me={me}
      canManage={permissions.isOwner || app.createdBy === me}
      requestedScreen={requestedScreen}
    />
  )
}
