import { useMemo } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  boardCollection,
} from "@/lib/collections"
import {
  useTeamBySlug,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  issueFiltersFromSearch,
  parseIssueFilterSearch,
  type IssueFilterSearch,
} from "@/lib/filters"
import { findIssuePosition } from "@/lib/board-view"
import type { Issue, IssueLabel, Board } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
)({
  // No route-level auth guard: the parent `/t/$teamSlug` layout route
  // (route.tsx) already gates access with public-team-aware logic —
  // anonymous visitors of a PUBLIC team pass through, while non-public /
  // inaccessible teams are redirected to login or 404'd there. Mirroring
  // the sibling board-view route, which likewise carries no beforeLoad, this
  // lets signed-out visitors open the read-only detail page (masterplan §4.3,
  // L29). The view renders read-only via `permissions.canMutateIssue` (false
  // when unauthenticated) and the comment/timeline UI is hidden for anonymous.
  //
  // Optional ?status/priority/labels mirror the board route's filter params —
  // navigating from a filtered board carries them here so the header's
  // prev/next switcher walks the board's exact filtered+sorted sequence, and
  // the board breadcrumb links back to the same filtered view. All params
  // are optional: links from my-issues / inbox / search arrive bare and fall
  // back to the unfiltered board ordering.
  validateSearch: (search: Record<string, unknown>): IssueFilterSearch =>
    parseIssueFilterSearch(search),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()
  const team = useTeamBySlug(teamSlug)

  const { data: boards } = useLiveQuery(
    (query) =>
      team
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) =>
              and(
                eq(boards.teamId, team.id),
                eq(boards.slug, boardSlug)
              )
            )
        : undefined,
    [team?.id, boardSlug]
  )
  const board = (boards?.[0] ?? null) as Board | null

  const { data: issues } = useLiveQuery(
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
  const issue = (issues?.[0] ?? null) as Issue | null

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
    (row) => row.labelId
  )

  // Same pipeline the board renders from (buildFilteredIssues →
  // buildVisibleIssueGroups over locally-synced rows — cheap), so the
  // switcher's ordering can never drift from the list the user came from.
  const filters = useMemo(
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )
  const { visibleGroups } = useBoardViewData({
    filters,
    boardSlug,
    teamSlug,
  })
  const position = issue ? findIssuePosition(visibleGroups, issue.id) : null
  const switcher = position
    ? {
        index: position.index,
        total: position.total,
        prevIdentifier: position.prev?.identifier ?? null,
        nextIdentifier: position.next?.identifier ?? null,
      }
    : null

  const { users } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team)

  if (!team || !board) {
    return (
      <div className="text-muted-foreground text-sm p-6">Loading…</div>
    )
  }

  if (!issue) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Issue <span className="font-mono">{issueIdentifier}</span> not found in
          this board.
        </div>
        <Link
          to="/t/$teamSlug/boards/$boardSlug"
          params={{ teamSlug, boardSlug }}
          className="text-foreground underline-offset-2 hover:underline"
        >
          ← Back to board
        </Link>
      </div>
    )
  }

  return (
    <IssueDetailView
      issue={issue}
      issueLabelIds={issueLabelIds}
      users={users}
      board={board}
      teamSlug={teamSlug}
      teamId={team.id}
      readOnly={!permissions.canMutateIssue(issue)}
      filterSearch={search}
      position={switcher}
    />
  )
}
