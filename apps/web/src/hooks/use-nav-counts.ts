import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  codingSessionCollection,
  issueCollection,
} from "@/lib/collections"
import type { CodingSession, Board } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots. Both count purely client-side over already-synced shapes.

// Open-PR count across the team's boards, matching the Reviews page's
// entry count: DISTINCT PRs, so a batch PR linked to several issues counts
// once (EXP-131).
export function useReviewsOpenPrCount(
  boards: Board[] | undefined
): number {
  const boardIds = useMemo(
    () => (boards ?? []).map((board) => board.id),
    [boards]
  )
  const { data } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.boardId, boardIds),
                eq(issues.prState, `open`)
              )
            )
        : undefined,
    [boardIds.join(`,`)]
  )
  return useMemo(() => {
    const keys = new Set<string>()
    for (const issue of data ?? []) {
      keys.add(issue.prUrl ?? issue.id)
    }
    return keys.size
  }, [data])
}

// Live count of live coding sessions in the team (team-scoped by the
// denormalized team_id) — running AND in_review (EXP-194: an agent awaiting
// review is exactly what the dot should pull attention to). Staleness guard
// (EXP-153): heartbeat-dead rows don't count.
export function useAgentsRunningCount(teamId?: string): number {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                inArray(sessions.status, [`running`, `in_review`])
              )
            )
        : undefined,
    [teamId]
  )
  const now = useNow()
  return ((data ?? []) as CodingSession[]).filter(
    (s) => !isCodingSessionStale(s.updatedAt, now)
  ).length
}
