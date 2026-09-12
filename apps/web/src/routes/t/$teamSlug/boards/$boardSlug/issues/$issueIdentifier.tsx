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
import type { Issue, IssueLabel } from "@/db/schema"
import { BoardNotFound } from "@/components/board-not-found"
import { IssueDetailView } from "@/components/issue-detail-view"

type IssueSearch = IssueFilterSearch & { from?: string }

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
  // navigating from a filtered board carries them here so a return to the
  // board lands on the same filtered view. All params are optional: links
  // from the inbox (either tab) / search arrive bare.
  //
  // EXP-851: `?from=` is the LIST this issue was opened from
  // (`lib/detail-origin.ts`) — the sidebar keeps it beside the issue, and
  // absent means the main menu stays.
  validateSearch: (search: Record<string, unknown>): IssueSearch => ({
    ...parseIssueFilterSearch(search),
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
  }),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()

  // The team/board/users lookups are the board view's, not a second copy
  // (EXP-791: the prev/next switcher that walked its sequence is gone).
  const filters = useMemo(
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )
  const { board, boardReady, team, users } = useBoardViewData({
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

  // EXP-851: the issue IS the content panel — the board list that used to sit
  // on its left moved into the sidebar's list nav (one list, one place, every
  // detail), which is what gives the description and timeline the full width.
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
      origin={search.from}
    />
  )
}
