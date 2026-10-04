import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Issue } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import {
  useTeamBySlug,
  useTeamBoardsWithReady,
} from "@/hooks/use-team-data"

// EXP-1154 retired the Reviews detail page: a review IS the issue's Changes
// face. This keeps old links alive: it finds the issue's board and replaces
// itself with `?view=diff`, or with the Reviews list for an unknown issue.
export const Route = createFileRoute(`/t/$teamSlug/reviews/$issueIdentifier`)({
  component: ReviewRedirect,
})

function ReviewRedirect() {
  const { teamSlug, issueIdentifier } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const boardIds = boards.map((board) => board.id).sort()

  const { data: issueRows, isReady: issuesReady } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.boardId, boardIds),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [boardIds.join(`,`), issueIdentifier]
  )
  const issue = (issueRows?.[0] ?? null) as Issue | null
  const boardSlug = issue
    ? boards.find((board) => board.id === issue.boardId)?.slug
    : undefined
  // Wait for the boards, then (when there are any) the issue lookup.
  const settled = boardsReady && (boardIds.length === 0 || issuesReady)

  useEffect(() => {
    if (boardSlug) {
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug, boardSlug, issueIdentifier },
        search: { from: `reviews`, view: `diff` },
        replace: true,
      })
    } else if (settled) {
      void navigate({
        to: `/t/$teamSlug/reviews`,
        params: { teamSlug },
        replace: true,
      })
    }
  }, [boardSlug, settled, teamSlug, issueIdentifier, navigate])

  return null
}
