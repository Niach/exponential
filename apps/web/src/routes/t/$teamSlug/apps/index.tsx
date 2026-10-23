import { createFileRoute, redirect } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { SkeletonRows } from "@exp/ui"
import { AppsList } from "@/components/apps/apps-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useApps } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle } from "@/lib/page-title"

// VAPP-110: the team's Apps (fixtures/apps.json): the management list.

export const Route = createFileRoute(`/t/$teamSlug/apps/`)({
  head: () => ({ meta: [{ title: pageTitle(APPS_COPY.title) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: AppsPage,
})

function AppsPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const { apps, isReady } = useApps(team?.id)

  if (!team || !isReady) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl">
        <SkeletonRows rowHeight={44} />
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}>
        <AppsList
          teamId={team.id}
          teamSlug={teamSlug}
          apps={apps}
          me={session?.user?.id ?? null}
          isOwner={permissions.isOwner}
        />
      </div>
    </div>
  )
}
