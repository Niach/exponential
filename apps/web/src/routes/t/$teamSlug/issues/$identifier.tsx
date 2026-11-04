import { createFileRoute, Navigate } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { LINK_NO_ACCESS } from "@/lib/app-link"
import { boardCollection, issueCollection } from "@/lib/collections"
import { useTeamBySlug } from "@/hooks/use-team-data"

// #EXP-1674: an issue by identifier alone (app-link.json `issueRef`): agents and
// app packages rarely know the board, so this resolves it and replaces the
// route with the board URL.
export const Route = createFileRoute(`/t/$teamSlug/issues/$identifier`)({
  component: IssueRefPage,
})

function IssueRefPage() {
  const { teamSlug, identifier } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const teamId = team?.id
  const { data: rows, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ issues: issueCollection })
            .innerJoin({ boards: boardCollection }, ({ issues, boards }) => eq(issues.boardId, boards.id))
            .where(({ issues, boards }) => and(eq(boards.teamId, teamId), eq(issues.identifier, identifier)))
            .select(({ boards }) => ({ boardSlug: boards.slug }))
        : undefined,
    [teamId, identifier]
  )
  const boardSlug = rows?.[0]?.boardSlug
  if (boardSlug) {
    return <Navigate replace to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier" params={{ teamSlug, boardSlug, issueIdentifier: identifier }} />
  }
  if (!team || !isReady) return <SkeletonRows />
  return <EmptyState icon={conceptIcon(`nav-search`)} title={LINK_NO_ACCESS} size="page" />
}
