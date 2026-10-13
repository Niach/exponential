import { useMemo } from "react"
import { useThreadsList } from "@/hooks/use-session-tree"
import { useMyLiveRuns } from "@/hooks/use-my-live-runs"
import { threadsNavState } from "@/components/team/sidebar-running"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots. Both count purely client-side over already-synced shapes.

/**
 * EXP-1293: the Threads entry's dot (the phone's Threads tab; md+ reads the
 * sidebar's own list) — green while any thread of the scope runs, amber
 * while one needs you. `teamIds` = the scope (the phone: every member team).
 */
export function useThreadsNav(teamIds: string | readonly string[] | undefined): {
  live: boolean
  needsYou: boolean
} {
  const { groups } = useThreadsList(teamIds)
  return useMemo(() => threadsNavState(groups), [groups])
}

// Live count of the signed-in user's OWN live sessions — `useMyLiveRuns`.
// The phone's chat arm shows a dot while any is live (EXP-880: never the
// count); `needsInput` (EXP-214) is true while any of them waits on a person
// — the badges escalate to amber for it.
export function useAgentsRunningCount(
  teamId?: string | readonly string[],
  currentUserId?: string
): {
  count: number
  needsInput: boolean
} {
  const { runs } = useMyLiveRuns(teamId, currentUserId)
  return {
    count: runs.length,
    needsInput: runs.some((session) => session.needsInput),
  }
}
