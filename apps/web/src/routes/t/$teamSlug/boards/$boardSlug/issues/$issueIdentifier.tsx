import { useMemo } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  issueFiltersFromSearch,
  parseIssueFilterSearch,
  type IssueFilterSearch,
} from "@/lib/filters"
import { findIssuePosition } from "@/lib/board-view"
import type { Issue, IssueLabel } from "@/db/schema"
import { BoardNotFound } from "@/components/board-not-found"
import { IssueDetailView } from "@/components/issue-detail-view"

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
)({
  // No route-level auth guard: the parent `/t/$teamSlug` layout route
  // (route.tsx) already gates access — anonymous or non-member requests are
  // redirected to login there (EXP-180: nothing is anonymously readable).
  // Mirroring the sibling board-view route, which likewise carries no
  // beforeLoad.
  //
  // Optional ?status/priority/labels mirror the board route's filter params —
  // navigating from a filtered board carries them here so the header's
  // prev/next switcher walks the board's exact filtered+sorted sequence, and
  // the board breadcrumb links back to the same filtered view. All params
  // are optional: links from the inbox (either tab) / search arrive bare and
  // fall back to the unfiltered board ordering.
  validateSearch: (search: Record<string, unknown>): IssueFilterSearch =>
    parseIssueFilterSearch(search),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()

  // Same pipeline the board renders from (buildFilteredIssues →
  // buildVisibleIssueGroups over locally-synced rows — cheap), so the
  // switcher's ordering can never drift from the list the user came from —
  // and the team/board/users lookups are the board view's, not a second copy.
  const filters = useMemo(
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )
  const { board, boardReady, team, users, visibleGroups } = useBoardViewData({
    filters,
    boardSlug,
    teamSlug,
  })

  const { data: issues, isReady: issuesQueryReady } = useLiveQuery(
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
  // A disabled live query reports `isReady: true`, so the board gate rides
  // along: on a cold deep link the issues snapshot always lands after the
  // boards one, and claiming "not found" in that window is a lie (REV2-32).
  const issueReady = Boolean(board) && issuesQueryReady

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

  const position = issue ? findIssuePosition(visibleGroups, issue.id) : null
  const switcher = position
    ? {
        index: position.index,
        total: position.total,
        prevIdentifier: position.prev?.identifier ?? null,
        nextIdentifier: position.next?.identifier ?? null,
      }
    : null

  const permissions = useTeamPermissions(team)

  if (!team || !board) {
    // Ready-and-empty boards means the slug is dead (trashed board, rename,
    // stale bookmark) — same recovery the board route offers (REV2-59).
    if (boardReady) {
      return (
        <BoardNotFound
          boardSlug={boardSlug}
          teamSlug={teamSlug}
        />
      )
    }
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  if (!issue) {
    // Absent-because-still-syncing, not absent-because-gone (REV2-32).
    if (!issueReady) {
      return <div className="text-muted-foreground text-sm p-6">Loading…</div>
    }
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Issue <span className="font-mono">{issueIdentifier}</span> not found
          in this board.
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
