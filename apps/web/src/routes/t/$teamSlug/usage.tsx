import { createFileRoute, redirect } from "@tanstack/react-router"
import { AgentUsagePage } from "@/components/agent-usage-page"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"

// EXP-792 (EXP-747 C1): every machine's agent usage on one page — mine and
// the servers shared with the team, one row per device × agent profile,
// attention first. Reached from the Devices page's section header; no
// sidebar entry of its own.
export const Route = createFileRoute(`/t/$teamSlug/usage`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: UsageRoute,
})

function UsageRoute() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const currentUserId = session?.user?.id

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }
  return (
    <AgentUsagePage
      teamSlug={teamSlug}
      teamId={team.id}
      currentUserId={currentUserId}
    />
  )
}
