import type { Team } from "@/db/schema"
import { useReviewsData } from "@/hooks/use-reviews-data"
import { reviewsNav } from "@/lib/reviews-queue"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { useMyLiveRuns } from "@/hooks/use-my-live-runs"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots. Both count purely client-side over already-synced shapes.

/**
 * EXP-1244: the Reviews nav entry (sidebar, rail, phone tab bar) reads the
 * SAME queue the page lists — `reviewsQueue` over the same synced rows and
 * the app-wide openPulls store, then `reviewsNav` (fixture-locked ×4): the
 * dot = anything to review, unlinked PRs included; EXP-1105 yolo mode hides
 * the entry unless something is open (an open PR there = a failed merge).
 * `teams` = the nav's scope (md+ the active team, the phone every member team).
 */
export function useReviewsNav(teams: readonly Team[]): {
  dot: boolean
  shows: boolean
} {
  const { count } = useReviewsData(undefined, teams)
  return reviewsNav({
    yolo: teams.map((team) => team.yoloMode === true),
    count,
  })
}

// Live count of the signed-in user's OWN live coding sessions in the team —
// `useMyLiveRuns` (EXP-870: the same set the work tabs auto-add). The Agent
// entry shows a dot while any is live (EXP-880: never the count). `needsInput` (EXP-214) is true
// while any live session sits on a plan-approval / AskUserQuestion picker —
// the badges escalate to amber for it.
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
    needsInput: runs.some(
      (s) => sessionDisplayState(s, null) === `needs_input`
    ),
  }
}
