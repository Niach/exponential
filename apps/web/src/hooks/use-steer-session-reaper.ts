import { useEffect, useMemo } from "react"
import { useParams } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { sessionCollection } from "@/lib/collections"
import { retainSteerSessions } from "@/lib/steer-session-store"

// The steer-store REAPER: the relay sockets of the caller's LIVE sessions,
// plus whatever session the route is showing, stay alive so navigating back
// resumes instantly; every other store without a subscriber is disposed after
// its grace period (`retainSteerSessions`). The team layout mounts this hook
// on EVERY breakpoint (a phone's sidebar Sheet never mounts). EXP-1293: the
// keep-set is my non-ended `sessions` rows of the active team.
export function useSteerSessionReaper(
  teamId: string | undefined,
  currentUserId: string | undefined
): void {
  const { data } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) =>
              and(eq(sessions.teamId, teamId), eq(sessions.userId, currentUserId))
            )
            .select(({ sessions }) => ({ id: sessions.id, status: sessions.status }))
        : undefined,
    [teamId, currentUserId]
  )
  const liveKey = useMemo(
    () =>
      ((data ?? []) as { id: string; status: string }[])
        .filter((row) => row.status !== `ended`)
        .map((row) => row.id)
        .sort()
        .join(`,`),
    [data]
  )
  const { sessionId: routeSessionId } = useParams({ strict: false })

  useEffect(() => {
    const keep = new Set(liveKey ? liveKey.split(`,`) : [])
    if (routeSessionId) keep.add(routeSessionId)
    retainSteerSessions(keep)
  }, [liveKey, routeSessionId])
}
