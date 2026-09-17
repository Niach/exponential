import { useMemo } from "react"
import { and, eq, inArray, or, useLiveQuery } from "@tanstack/react-db"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { codingSessionCollection } from "@/lib/collections"
import type { CodingSession } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import { isLiveRun } from "@/lib/past-runs"

// EXP-870: the signed-in user's OWN live runs in the team — running and
// in_review (EXP-194), heartbeat-dead rows dropped (EXP-153). Every run
// counts: person-started, automations, remote devices. ONE definition behind
// the Agent entry's count badge (`useAgentsRunningCount`) and the work tabs'
// auto-added live tabs, so the badge and the strip never disagree. Own-only
// to match the owner-only session views (EXP-312).
export function useMyLiveRuns(
  teamId: string | undefined,
  currentUserId: string | undefined,
  nowIntervalMs = 60_000
): { runs: CodingSession[]; isReady: boolean; now: Date } {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
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
    [teamId, currentUserId]
  )
  const now = useNow(nowIntervalMs)
  const runs = useMemo(
    () =>
      ((data ?? []) as CodingSession[])
        .filter(
          (session) =>
            isLiveRun(session) && !isCodingSessionStale(session.updatedAt, now)
        )
        .sort(
          (left, right) =>
            new Date(left.startedAt).getTime() -
              new Date(right.startedAt).getTime() ||
            left.id.localeCompare(right.id)
        ),
    [data, now]
  )
  return { runs, isReady: Boolean(teamId && currentUserId) && isReady, now }
}
