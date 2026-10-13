import { useMemo } from "react"
import { and, eq, inArray, not, useLiveQuery } from "@tanstack/react-db"
import { sessionCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
import { teamScopeIds } from "@/lib/team-scope"

// EXP-870 / EXP-1293: the signed-in user's OWN live sessions in the team(s)
// — every `sessions` row they started that has not ended (queued, running or
// idle; roots and children alike). Behind the phone chat arm's dot
// (`useAgentsRunningCount`). Own-only to match the owner-only live views
// (EXP-312).
export function useMyLiveRuns(
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined
): { runs: SessionRow[]; isReady: boolean } {
  const teamIds = teamScopeIds(teamId)
  const teamKey = teamIds.join(`,`)
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0 && currentUserId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) =>
              and(
                inArray(sessions.teamId, teamIds),
                eq(sessions.userId, currentUserId),
                not(eq(sessions.status, `ended`))
              )
            )
        : undefined,
    [teamKey, currentUserId]
  )
  const runs = useMemo(
    () =>
      ((data ?? []) as SessionRow[]).sort(
        (left, right) =>
          new Date(left.createdAt).getTime() - new Date(right.createdAt).getTime() ||
          left.id.localeCompare(right.id)
      ),
    [data]
  )
  return {
    runs,
    isReady: Boolean(teamIds.length > 0 && currentUserId) && isReady,
  }
}
