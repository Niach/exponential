import { useMemo } from "react"
import { and, eq, not, useLiveQuery } from "@tanstack/react-db"
import { sessionCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
import {
  liveRunsByTeam,
  type TeamLiveRuns,
} from "@/lib/sessions/team-live-runs"

// EXP-1075: the caller's own live sessions across EVERY team they sync,
// grouped by team — the team picker's dot. Team-blind on purpose: it answers
// "where else is something of mine alive".
export function useTeamLiveRuns(
  currentUserId: string | undefined
): Map<string, TeamLiveRuns> {
  const { data } = useLiveQuery(
    (query) =>
      currentUserId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) =>
              and(eq(sessions.userId, currentUserId), not(eq(sessions.status, `ended`)))
            )
        : undefined,
    [currentUserId]
  )
  return useMemo(
    () =>
      currentUserId
        ? liveRunsByTeam((data ?? []) as SessionRow[], { me: currentUserId })
        : new Map<string, TeamLiveRuns>(),
    [data, currentUserId]
  )
}
