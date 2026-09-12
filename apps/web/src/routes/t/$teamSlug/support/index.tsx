import { createFileRoute, redirect } from "@tanstack/react-router"
import { SupportThreadList } from "@/components/helpdesk/support-inbox"
import { useTeamBySlug } from "@/hooks/use-team-data"

// The helpdesk member inbox (EXP-128). The sidebar links here only when the
// team has helpdesk_enabled, but the route itself just renders empty lists
// otherwise — the server-side member gate on the helpdesk router is the
// boundary.
//
// EXP-851: a LIST, nothing else — the Open/Resolved strip over the threads. A
// conversation is its own route (`support/$threadId`) with this list in the
// sidebar beside it, so a ticket is a URL you can share and refresh.
export const Route = createFileRoute(`/t/$teamSlug/support/`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
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

  return <SupportThreadList teamId={team.id} teamSlug={teamSlug} />
}
