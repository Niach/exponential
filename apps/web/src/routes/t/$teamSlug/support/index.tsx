import { createFileRoute, redirect } from "@tanstack/react-router"
import { SupportInbox } from "@/components/helpdesk/support-inbox"
import { useTeamBySlug } from "@/hooks/use-team-data"

// The helpdesk member inbox (EXP-128): a 3-pane Featurebase-style view over
// the team's support threads. The sidebar links here only when the
// team has helpdesk_enabled, but the route itself just renders empty
// lists otherwise — the server-side member gate on the helpdesk router is the
// boundary.
export const Route = createFileRoute(`/t/$teamSlug/support/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: SupportPage,
})

function SupportPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)

  if (!team) {
    return (
      <div className="p-6 text-sm text-muted-foreground">Loading…</div>
    )
  }

  return (
    <SupportInbox teamId={team.id} teamSlug={teamSlug} />
  )
}
