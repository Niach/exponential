import { useCallback } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { IssueDetailView } from "@/components/issue-detail-view"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { issueLabelCollection } from "@/lib/collections"
import type { IssueLabel } from "@/db/schema"
import { useSessionRow } from "@/hooks/use-agents-data"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-827: a run's linked ISSUE, opened from the session's issue band — the
// content panel swaps the transcript for the issue detail while the sidebar
// keeps the run's own list nav (EXP-851, `?from=`). A NON-NESTED route
// (`$sessionId_`): the session page is a leaf, not a layout, so this is its
// sibling under the same URL prefix. The header's back returns to the session.

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId_/issue`)({
  // EXP-818: the run's origin rides through this hop so the session's own Back
  // still knows where the reader came from.
  validateSearch: (search: Record<string, unknown>): { from?: string } => ({
    from: typeof search.from === `string` && search.from ? search.from : undefined,
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SessionIssuePage,
})

function SessionIssuePage() {
  const { teamSlug, sessionId } = Route.useParams()
  const { from } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const { row, isReady } = useSessionRow(team?.id, currentUserId, sessionId)
  const { users } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team)

  const issue = row?.issue ?? null
  const board = row?.board ?? null
  const { data: issueLabels } = useLiveQuery(
    (query) =>
      issue
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) => eq(issueLabels.issueId, issue.id))
        : undefined,
    [issue?.id]
  )
  const issueLabelIds = ((issueLabels ?? []) as IssueLabel[]).map(
    (label) => label.labelId
  )

  const backToSession = useCallback(() => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug, sessionId },
      search: from ? { from } : {},
    })
  }, [navigate, teamSlug, sessionId, from])

  if (!team || !currentUserId || !isReady) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="p-6 text-sm text-muted-foreground">Loading…</div>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* EXP-851: the shared detail header — round back to the run, the
          issue's identifier centred. */}
      <MobileDetailHeader
        title={issue ? issue.identifier : `Issue`}
        onBack={backToSession}
        backLabel="Back to session"
      />
      {issue && board ? (
        <div className="min-h-0 flex-1">
          <IssueDetailView
            key={issue.id}
            issue={issue}
            issueLabelIds={issueLabelIds}
            users={users}
            board={board}
            teamSlug={teamSlug}
            teamId={team.id}
            readOnly={!permissions.canMutateIssue(issue)}
            showMobileHeader={false}
          />
        </div>
      ) : (
        <div className="p-6 text-center text-sm text-muted-foreground">
          {row ? `This run has no issue.` : `Session not found.`}
        </div>
      )}
    </div>
  )
}
