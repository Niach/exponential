import { useEffect } from "react"
import { useParams } from "@tanstack/react-router"
import { useAgentsData } from "@/hooks/use-agents-data"
import { retainSteerSessions } from "@/lib/steer-session-store"

// The steer-store REAPER: the relay sockets of the caller's RUNNING sessions,
// plus whatever session the route is showing, stay alive so navigating back
// resumes instantly; every other store without a subscriber is disposed after
// its grace period (`retainSteerSessions`). It rode the dock (EXP-740), then
// the sidebar's Sessions group (EXP-818) — but on phones the sidebar is a
// Sheet that never opens, so that group never mounted and every opened
// session kept its socket until reload. The team layout mounts this hook on
// EVERY breakpoint; the sidebar group is render-only.
export function useSteerSessionReaper(
  teamId: string | undefined,
  currentUserId: string | undefined
): void {
  const { running } = useAgentsData(teamId, currentUserId)
  const { sessionId: routeSessionId } = useParams({ strict: false })

  useEffect(() => {
    const keep = new Set(running.map((row) => row.session.id))
    if (routeSessionId) keep.add(routeSessionId)
    retainSteerSessions(keep)
  }, [running, routeSessionId])
}
