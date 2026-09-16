import { createFileRoute, Navigate, redirect } from "@tanstack/react-router"
import { useSessionRow } from "@/hooks/use-agents-data"
import { issueCollection } from "@/lib/collections"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"

// EXP-870: LEGACY REDIRECT. EXP-827 opened a run's linked issue on a path
// nested under the run; EXP-870 made the issue and its run ONE work tab with
// two faces, so the issue face is simply the issue's canonical URL. `?from=`
// rides along. A run with no issue (or no run at all) lands back on the run's
// own page. Nothing renders but a loading line while the collections sync.
export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId_/issue`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
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
  component: SessionIssueRedirect,
})

function SessionIssueRedirect() {
  const { teamSlug, sessionId } = Route.useParams()
  const { from } = Route.useSearch()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const { row, isReady } = useSessionRow(team?.id, currentUserId, sessionId)
  const search = from ? { from } : {}

  if (row?.issue && row.board) {
    return (
      <Navigate
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{
          teamSlug,
          boardSlug: row.board.slug,
          issueIdentifier: row.issue.identifier,
        }}
        search={search}
        replace
      />
    )
  }
  // Settled without an issue to show — an issue-less run, an unknown id, or
  // an issue that is gone once the issues shape is ready — back to the run's
  // own page. A row that names an issue waits for it only while it may still
  // be syncing.
  const waitingOnIssue =
    Boolean(row?.session.issueId) && !row?.issue && !issueCollection.isReady()
  if (team && currentUserId && isReady && !waitingOnIssue) {
    return (
      <Navigate
        to="/t/$teamSlug/sessions/$sessionId"
        params={{ teamSlug, sessionId }}
        search={search}
        replace
      />
    )
  }
  return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
}
