import { useMemo } from "react"
import { createFileRoute, Navigate, redirect } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { issueSessionTarget } from "@/hooks/use-open-session"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useSession } from "@/hooks/use-session"
import type { CodingSession, Issue } from "@/db/schema"
import { pageTitle } from "@/lib/page-title"

// EXP-870: LEGACY REDIRECT. EXP-851 put an issue's run on the issue's own URL;
// EXP-870 made a run ONE URL (`/t/$teamSlug/sessions/$sessionId`) with the
// issue as the same work tab's other face. Links and bookmarks to this path
// still resolve: the run `?run=` names (EXP-856) when it belongs to this
// issue, else the issue's newest own run — `issueSessionTarget` — and the
// issue itself when it has none. `?from=` rides along. Nothing renders but a
// loading line while the collections sync.
export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier_/session`
)({
  head: ({ params }) => ({
    meta: [{ title: pageTitle(params.issueIdentifier) }],
  }),
  validateSearch: (
    search: Record<string, unknown>
  ): { from?: string; run?: string } => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
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
  component: IssueSessionRedirect,
})

function IssueSessionRedirect() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const { from, run } = Route.useSearch()
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id

  const { board, boardReady } = useBoardViewData({ boardSlug, teamSlug })
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
  const search = from ? { from } : {}

  if (target) {
    return (
      <Navigate
        to="/t/$teamSlug/sessions/$sessionId"
        params={{ teamSlug, sessionId: target.id }}
        search={search}
        replace
      />
    )
  }
  // Settled with no run (or no issue / board at all): the issue's canonical
  // URL, which owns the not-found states.
  const settled =
    currentUserId !== undefined &&
    boardReady &&
    (!board || (Boolean(board) && issuesReady && (!issue || sessionsReady)))
  if (settled) {
    return (
      <Navigate
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{ teamSlug, boardSlug, issueIdentifier }}
        search={search}
        replace
      />
    )
  }
  return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
}
