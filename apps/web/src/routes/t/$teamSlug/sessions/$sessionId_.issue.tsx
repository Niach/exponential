import { useCallback } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { AgentShell } from "@/components/agent-shell"
import { IssueDetailView } from "@/components/issue-detail-view"
import { Button } from "@/components/ui/button"
import { conceptIcon } from "@/lib/icons.generated"
import { issueLabelCollection } from "@/lib/collections"
import type { IssueLabel } from "@/db/schema"
import { useSessionRow } from "@/hooks/use-agents-data"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-827: a run's linked ISSUE, opened from the session's issue band, inside
// the Agent shell — the sessions list on the left stays, the right pane swaps
// the transcript for the issue detail. A NON-NESTED route (`$sessionId_`):
// the session page is a leaf, not a layout, so this is its sibling under the
// same URL prefix. The header's back returns to the session.

const UiBackIcon = conceptIcon(`ui-back`)

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
    <AgentShell teamId={team.id} currentUserId={currentUserId} activeSessionId={sessionId}>
      <div className="flex h-full min-h-0 flex-col">
        <div className="flex items-center gap-1 border-b border-border px-1 py-1.5">
          <Button
            variant="ghost"
            size="icon"
            className="shrink-0"
            aria-label="Back to session"
            title="Back to session"
            onClick={backToSession}
          >
            <UiBackIcon />
          </Button>
          <span className="min-w-0 flex-1 truncate text-center text-sm font-medium">
            {issue ? issue.identifier : `Issue`}
          </span>
          {/* Balances the back button so the title stays optically centred. */}
          <span className="size-9 shrink-0" />
        </div>
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
            />
          </div>
        ) : (
          <div className="p-6 text-center text-sm text-muted-foreground">
            {row ? `This run has no issue.` : `Session not found.`}
          </div>
        )}
      </div>
    </AgentShell>
  )
}
