import { useEffect, useMemo } from "react"
import { useParams } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { sessionCollection, teamCollection } from "@/lib/collections"
import { retainSteerSessions } from "@/lib/steer-session-store"

// The steer-store REAPER: the relay sockets of the caller's LIVE sessions,
// plus whatever session the route is showing, stay alive so navigating back
// resumes instantly; every other store without a subscriber is disposed after
// its grace period (`retainSteerSessions`). The team layout mounts this hook
// on EVERY breakpoint (a phone's sidebar Sheet never mounts). EXP-1293: the
// keep-set is my non-ended `sessions` rows of the active team, plus mine in
// a team I do not sync (#EXP-2079).
export function useSteerSessionReaper(
  teamId: string | undefined,
  currentUserId: string | undefined
): void {
  // #EXP-2079 (agent-settings.json `assistant.chatTeams` LIVE STATE): the
  // viewer's own session in a team they do not sync (the Assistant's
  // conversation in their personal team) stays kept on every team's page.
  const { data: mine } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => eq(sessions.userId, currentUserId))
            .select(({ sessions }) => ({ id: sessions.id, status: sessions.status, teamId: sessions.teamId }))
        : undefined,
    [teamId, currentUserId]
  )
  const { data: teams } = useLiveQuery((query) => query.from({ teams: teamCollection }).select(({ teams }) => ({ id: teams.id })))
  const data = useMemo(() => {
    const synced = new Set(((teams ?? []) as { id: string }[]).map((team) => team.id))
    return ((mine ?? []) as { id: string; status: string; teamId: string }[]).filter(
      (row) => row.teamId === teamId || !synced.has(row.teamId)
    )
  }, [mine, teams, teamId])
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
