import { createFileRoute, redirect } from "@tanstack/react-router"
import { SkeletonRows } from "@exp/ui"
import { TeamActionsPanel } from "@/components/team-actions-panel"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"

// The team Actions surface (EXP-686 — its own top-level route on every
// client): the actions LIST (EXP-1293: saved prompts) on every viewport. An
// action itself is a page: `actions/$actionId`.

export const Route = createFileRoute(`/t/$teamSlug/actions/`)({
  head: () => ({ meta: [{ title: pageTitle(`Actions`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ActionsPage,
})

function ActionsPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  // EXP-1186: the phone lists every member team's actions, one band per
  // team; md+ = the active team.
  const scope = useCrossTeamScope(team)

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto flex min-h-full w-full max-w-3xl flex-col px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <TeamActionsPanel
            team={team}
            teams={scope.teams}
            grouped={scope.grouped}
          />
        ) : (
          // D8 first load: the ActionRow geometry (ListRow 44px, icon).
          <SkeletonRows />
        )}
      </div>
    </div>
  )
}
