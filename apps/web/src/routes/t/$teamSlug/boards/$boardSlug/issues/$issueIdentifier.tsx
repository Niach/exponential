import { createFileRoute, Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { SkeletonRows } from "@exp/ui"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { Issue } from "@/db/schema"
import { BoardNotFound } from "@/components/board-not-found"
import { IssueDetailView } from "@/components/issue-detail-view"
import { pageTitle, usePageTitle } from "@/lib/page-title"

type IssueSearch = { from?: string; comment?: string }

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
)({
  head: ({ params }) => ({
    meta: [
      { title: pageTitle(params.issueIdentifier) },
      // FEED-88: the identifier only: unfurlers are anonymous.
      { property: `og:title`, content: pageTitle(params.issueIdentifier) },
    ],
  }),
  // No route-level auth guard: the parent `/t/$teamSlug` layout route
  // (route.tsx) already gates access — anonymous or non-member requests are
  // redirected to login there (EXP-180: nothing is anonymously readable).
  // Mirroring the sibling board-view route, which likewise carries no
  // beforeLoad.
  //
  // EXP-851: `?from=` is the LIST this issue was opened from
  // (`lib/detail-origin.ts`) — the sidebar keeps it beside the issue, and
  // absent means the main menu stays.
  // EXP-1241: `?comment=<id>` scrolls the timeline to that comment and briefly
  // highlights it (`IssueTimeline`).
  // EXP-1293: ONE face. No Issue|Session|Guide toggle and no `?view=` /
  // `section` / `file` params: a session is its thread's, the Guide too.
  validateSearch: (search: Record<string, unknown>): IssueSearch => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
    comment:
      typeof search.comment === `string` && search.comment
        ? search.comment
        : undefined,
  }),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()

  // The team/board/users lookups are the board view's, not a second copy
  // (EXP-791: the prev/next switcher that walked its sequence is gone).
  const { board, boardReady, team, users } = useBoardViewData({
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
  usePageTitle(
    issue ? pageTitle(`${issue.identifier} ${issue.title}`) : undefined
  )
  // A disabled live query reports `isReady: true`, so the board gate rides
  // along: on a cold deep link the issues snapshot always lands after the
  // boards one, and claiming "not found" in that window is a lie (REV2-32).
  const issueReady = Boolean(board) && issuesQueryReady

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
    return <IssueDetailPlaceholder />
  }

  if (!issue) {
    // Absent-because-still-syncing, not absent-because-gone (REV2-32).
    if (!issueReady) {
      return <IssueDetailPlaceholder />
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

  const readOnly = !permissions.canMutateIssue(issue)

  // EXP-851: the issue IS the content panel, on every layout (the phone's
  // header + floating bar are the view's own).
  return (
    <IssueDetailView
      issue={issue}
      users={users}
      board={board}
      teamSlug={teamSlug}
      teamId={team.id}
      readOnly={readOnly}
      origin={search.from}
    />
  )
}

/** D8 first load: the detail page's placeholder (3 list rows in its padding). */
function IssueDetailPlaceholder() {
  return (
    <div className="p-6">
      <SkeletonRows />
    </div>
  )
}
