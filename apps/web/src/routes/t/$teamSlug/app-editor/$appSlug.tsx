import { useEffect, useRef, useState } from "react"
import { useMemo } from "react"
import { createFileRoute, redirect, useNavigate, useParams, useSearch } from "@tanstack/react-router"
import { APPS_COPY, APPS_EDITOR_TABS, APPS_PREVIEW, screenPath } from "@exp/domain-contract/apps"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { AppEditor } from "@/components/apps/app-editor"
import { useApp, useApps } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { routeScreenParams, type ScreenRequest } from "@/lib/app-screens"

// VAPP-110: one app's EDITOR (fixtures/apps.json): chat beside the live
// preview, versions. `$appSlug` = the app's slug in the team. VAPP-176: a
// draft's slug follows its new name, so the page keys the app by ID and
// replaces the route in place (the editor never remounts: that would end
// the editor session mid-turn). VAPP-177: `?tab=<id>` opens that tab (phones:
// versions/settings = the menu's sheet over Chat; `editorTab`). VAPP-179:
// `?frame=desktop|phone` opens the preview in that frame (screenshots).
// VAPP-218: the child route `$appSlug/$screen` (its query = the screen's
// params, apps.json `screens`) opens the Preview tab on that screen; this
// page stays the one editor for both, so a screen change never remounts it.

const NavAppsIcon = conceptIcon(`nav-apps`)
const SYNC_GRACE_MS = 4_000

type AppEditorSearch = { tab?: string; frame?: `desktop` | `phone` }

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug`)({
  validateSearch: (search: Record<string, unknown>): AppEditorSearch => {
    const tab = search[APPS_EDITOR_TABS.search]
    const frame = search.frame
    return {
      ...(typeof tab === `string` && tab ? { tab } : {}),
      ...(typeof frame === `string` && (APPS_PREVIEW.frames as string[]).includes(frame) ? { frame: frame as `desktop` | `phone` } : {}),
    }
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
  const { tab: requestedTab, frame: requestedFrame } = Route.useSearch()
  // VAPP-218: the screen route's `$screen` + its query (the child's own match).
  const { screen } = useParams({ strict: false }) as { screen?: string }
  const search = useSearch({ strict: false }) as Record<string, unknown>
  const paramsKey = screen ? JSON.stringify(routeScreenParams(search)) : ``
  const requestedScreen = useMemo<ScreenRequest | null>(
    () => (screen ? { screen, params: JSON.parse(paramsKey) as Record<string, string> } : null),
    [screen, paramsKey]
  )
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
    <AppEditor
      key={app.id}
      app={app}
      teamId={team.id}
      teamSlug={teamSlug}
      me={session?.user?.id ?? null}
      isMember={permissions.isMember}
      canManage={permissions.isOwner || app.createdBy === (session?.user?.id ?? null)}
      canGrant={permissions.isOwner}
      requestedTab={screen ? `preview` : (requestedTab ?? null)}
      requestedFrame={requestedFrame ?? null}
      requestedScreen={requestedScreen}
    />
  )
}
