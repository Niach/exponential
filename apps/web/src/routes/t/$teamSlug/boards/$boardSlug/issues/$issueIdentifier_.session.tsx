import { useCallback, useMemo } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { AgentSessionView } from "@/components/agent-session"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { Button } from "@/components/ui/button"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { issueSessionTarget } from "@/hooks/use-open-session"
import { emptyFilters } from "@/lib/filters"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { rowPrState, useSessionRow } from "@/hooks/use-agents-data"
import { useSession } from "@/hooks/use-session"
import { sessionIdentity } from "@/lib/session-identity"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-851: an ISSUE's coding run, on the issue's own URL. Starting a run from
// an issue detail (or opening the run it already has) lands here instead of
// the flat `/sessions/$sessionId`: the sidebar keeps the board's list nav, the
// header's back returns to the issue, and the run is one hop from the work it
// belongs to.
//
// Which run: the one `?run=` NAMES when it belongs to this issue (EXP-856 —
// the issue can have several, and the row that was clicked is the one the
// reader meant), otherwise the issue's NEWEST OWN one (live or ended), the
// same rule the issue detail's Watch pill follows (`issue-coding-rows.tsx`).
// EXP-312 keeps live runs owner-only, so a teammate's run never mounts a view
// here.
//
// A NON-NESTED route (`$issueIdentifier_`): the issue page is a leaf, not a
// layout, so this is its sibling under the same URL prefix.
export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier_/session`
)({
  validateSearch: (
    search: Record<string, unknown>
  ): { from?: string; run?: string } => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
    // EXP-856: the session id this page was opened ON. Ignored when it names
    // a run of another issue (the query below only holds this issue's).
    run: typeof search.run === `string` && search.run ? search.run : undefined,
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: IssueSessionPage,
})

function IssueSessionPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const { from, run } = Route.useSearch()
  const navigate = useNavigate()
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id

  const { board, boardReady, team } = useBoardViewData({
    filters: emptyFilters,
    boardSlug,
    teamSlug,
  })

  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      board
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                eq(issues.boardId, board.id),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [board?.id, issueIdentifier]
  )
  const issue = ((issues ?? []) as Issue[])[0] ?? null

  // Every run this issue has had — the query IS the belonging check, so a
  // `?run=` naming another issue's session simply finds nothing here and the
  // page falls back to the newest of the CALLER's own.
  const { data: sessionRows, isReady: sessionsReady } = useLiveQuery(
    (query) =>
      issue
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.issueId, issue.id))
        : undefined,
    [issue?.id]
  )
  const target = useMemo(
    () =>
      issueSessionTarget(
        (sessionRows ?? []) as CodingSession[],
        run,
        currentUserId
      ),
    [sessionRows, run, currentUserId]
  )

  const { row, session, isReady } = useSessionRow(
    team?.id,
    currentUserId,
    target?.id
  )

  const goBack = useCallback(() => {
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug, boardSlug, issueIdentifier },
      search: from ? { from } : {},
    })
  }, [navigate, teamSlug, boardSlug, issueIdentifier, from])

  if (!team || !currentUserId || (!board && !boardReady)) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  if (!issue) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <MobileDetailHeader title={issueIdentifier} onBack={goBack} />
        <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">
          {issuesReady && boardReady
            ? `Issue ${issueIdentifier} not found in this board.`
            : `Loading…`}
        </div>
      </div>
    )
  }

  if (!target || !session || !row) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <MobileDetailHeader title={issue.identifier} onBack={goBack} />
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-sm text-muted-foreground">
            {sessionsReady && isReady
              ? `No coding run on this issue yet.`
              : `Loading…`}
          </p>
          <Button variant="outline" size="sm" onClick={goBack}>
            Back to issue
          </Button>
        </div>
      </div>
    )
  }

  const identity = sessionIdentity(row)

  // EXP-312: only the owner may steer a run — this is the gate, not a belt:
  // `?run=` may name a TEAMMATE's run on this issue, which renders the synced
  // badge instead of minting a ticket.
  if (session.userId !== currentUserId) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <MobileDetailHeader title={issue.identifier} onBack={goBack} />
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <SessionStatusBadge
            session={session}
            prState={rowPrState(session, row.issue)}
          />
          <p className="text-sm text-muted-foreground">
            Only the owner can steer this session.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <AgentSessionView
        key={session.id}
        session={session}
        currentUserId={currentUserId}
        identity={identity}
        mergeTarget={row.mergeTarget}
        issue={row.issue ?? issue}
        onOpenIssue={goBack}
        onBack={goBack}
      />
    </div>
  )
}
