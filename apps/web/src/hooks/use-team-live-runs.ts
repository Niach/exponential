import { useMemo } from "react"
import { and, eq, isNull, not, useLiveQuery } from "@tanstack/react-db"
import { sessionCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
import {
  liveRunsByTeam,
  type TeamLiveRuns,
} from "@/lib/sessions/team-live-runs"

/** The ids of every synced agent CONVERSATION root (NEW-2: a root with an
 *  `agentId`): sessions under them never count as live
 *  (`countsAsLiveSession`). */
export function useAgentRootIds(): ReadonlySet<string> {
  const { data } = useLiveQuery((query) =>
    query
      .from({ sessions: sessionCollection })
      .where(({ sessions }) => isNull(sessions.parentId))
      .select(({ sessions }) => ({ id: sessions.id, agentId: sessions.agentId }))
  )
  return useMemo(
    () =>
      new Set(
        ((data ?? []) as { id: string; agentId: string | null }[])
          .filter((row) => row.agentId)
          .map((row) => row.id)
      ),
    [data]
  )
}

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
  const agentRootIds = useAgentRootIds()
  return useMemo(
    () =>
      currentUserId
        ? liveRunsByTeam((data ?? []) as SessionRow[], { me: currentUserId, agentRootIds })
        : new Map<string, TeamLiveRuns>(),
    [data, currentUserId, agentRootIds]
  )
}
