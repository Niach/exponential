import { useCallback } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { SupportConversation } from "@/components/helpdesk/support-inbox"
import { useTeamBySlug } from "@/hooks/use-team-data"

// EXP-851: ONE helpdesk conversation, on its own route. The 3-pane inbox is
// gone — the thread list lives in the sidebar's list nav (`?from=support`),
// the conversation fills the content panel, and the ticket's details stay in
// the lg+ rail / the sheet behind the header's info button.
export const Route = createFileRoute(`/t/$teamSlug/support/$threadId`)({
  // The origin the list handed this detail (`lib/detail-origin.ts`) — absent
  // on a shared link, which is exactly the "main menu stays" case.
  validateSearch: (search: Record<string, unknown>): { from?: string } => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SupportThreadPage,
})

function SupportThreadPage() {
  const { teamSlug, threadId } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)

  const goBack = useCallback(() => {
    void navigate({ to: `/t/$teamSlug/support`, params: { teamSlug } })
  }, [navigate, teamSlug])

  if (!team) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  return (
    <SupportConversation
      key={threadId}
      threadId={threadId}
      teamId={team.id}
      teamSlug={teamSlug}
      onBack={goBack}
    />
  )
}
