import { createFileRoute, redirect } from "@tanstack/react-router"
import { SkeletonRows } from "@exp/ui"
import { ActionPage } from "@/components/action-page"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"

// ONE action (EXP-1293: a saved prompt) — its header and its one form, the
// same on every viewport.

export const Route = createFileRoute(`/t/$teamSlug/actions/$actionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Actions`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ActionDetailPage,
})

function ActionDetailPage() {
  const { teamSlug, actionId } = Route.useParams()
  const team = useTeamBySlug(teamSlug)

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto flex min-h-full w-full max-w-3xl flex-col px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <ActionPage
            key={actionId}
            team={team}
            teamSlug={teamSlug}
            actionId={actionId}
          />
        ) : (
          // D8 first load: the same placeholder ActionPage draws.
          <div className="p-6">
            <SkeletonRows />
          </div>
        )}
      </div>
    </div>
  )
}
