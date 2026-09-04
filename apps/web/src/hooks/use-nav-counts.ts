import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  codingSessionCollection,
  issueCollection,
} from "@/lib/collections"
import type { CodingSession, Board } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { useNow } from "@/hooks/use-now"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots. Both count purely client-side over already-synced shapes.

// Open-PR count across the team's boards, matching the Reviews page's
// entry count: DISTINCT PRs, so a batch PR linked to several issues counts
// once (EXP-131). EXP-734: plus the run PRs that link no issue at all — an
// action or chat run stamps its own prUrl on the session row, and Reviews
// lists those under "Agent runs".
export function useReviewsOpenPrCount(
  boards: Board[] | undefined,
  teamId?: string
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
  const { data: sessionData } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.prState, `open`)
              )
            )
        : undefined,
    [teamId]
  )
  return useMemo(() => {
    // One key space: a run PR's url can never sit on an issue row, and
    // keying both on prUrl dedupes either way.
    const keys = new Set<string>()
    for (const issue of data ?? []) {
      keys.add(issue.prUrl ?? issue.id)
    }
    for (const session of (sessionData ?? []) as CodingSession[]) {
      if (session.issueId != null || !session.prUrl) continue
      keys.add(session.prUrl)
    }
    return keys.size
  }, [data, sessionData])
}

// Live count of the signed-in user's OWN live coding sessions in the team —
// running AND in_review (EXP-194: an agent awaiting review is exactly
// what the dot should pull attention to). Own-only to match the owner-only Agents list: a
// teammate's session must not light a badge over a list that shows nothing.
// Staleness guard (EXP-153): heartbeat-dead rows don't count. `needsInput`
// (EXP-214) is true while any live session sits on a plan-approval /
// AskUserQuestion picker — the badges escalate to amber for it.
export function useAgentsRunningCount(
  teamId?: string,
  currentUserId?: string
): {
  count: number
  needsInput: boolean
} {
  const { data } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.userId, currentUserId),
                inArray(sessions.status, [`running`, `in_review`])
              )
            )
        : undefined,
    [teamId, currentUserId]
  )
  const now = useNow()
  const live = ((data ?? []) as CodingSession[]).filter(
    (s) => !isCodingSessionStale(s.updatedAt, now)
  )
  return {
    count: live.length,
    needsInput: live.some(
      (s) => sessionDisplayState(s, null) === `needs_input`
    ),
  }
}
