import { useMemo } from "react"
import { and, eq, inArray, or, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection } from "@/lib/collections"
import type { CodingSession } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import {
  liveRunsByTeam,
  type TeamLiveRuns,
} from "@/lib/sessions/team-live-runs"

// EXP-1075: the caller's own live runs across EVERY team they sync, grouped
// by team — the team picker's dot. Team-blind on purpose: `useMyLiveRuns`
// answers "what is running HERE", this one answers "where else is something
// of mine alive".
export function useTeamLiveRuns(
  currentUserId: string | undefined,
  nowIntervalMs = 60_000
): Map<string, TeamLiveRuns> {
  const { data } = useLiveQuery(
    (query) =>
      currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.userId, currentUserId),
                // EXP-888: a SWEPT row (`ended` + `ended_by = 'stale'`) is
                // still live — the device ignores the flip and its next
                // heartbeat revives it — so it stays in the strip.
                or(
                  inArray(sessions.status, [`running`, `in_review`]),
                  and(
                    eq(sessions.status, `ended`),
                    eq(sessions.endedBy, `stale`)
                  )
                )
              )
            )
        : undefined,
    [currentUserId]
  )
  const now = useNow(nowIntervalMs)
  return useMemo(
    () =>
      currentUserId
        ? liveRunsByTeam((data ?? []) as CodingSession[], {
            me: currentUserId,
            now,
          })
        : new Map<string, TeamLiveRuns>(),
    [data, now, currentUserId]
  )
}
