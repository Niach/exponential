import { createFileRoute, redirect } from "@tanstack/react-router"
import { DraftsList } from "@/components/drafts-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useTeamBySlug } from "@/hooks/use-team-data"

// EXP-878: the team's Drafts surface on web ≥md — reached from the sidebar
// entry, which only appears while the caller HAS a draft. Below md there is
// no route: the Inbox grows a third tab instead, so a phone never navigates
// to a page that can empty itself out from under it.
export const Route = createFileRoute(`/t/$teamSlug/drafts`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: DraftsPage,
})

function DraftsPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        <DraftsList teamId={team?.id} teamSlug={teamSlug} />
      </div>
    </div>
  )
}
