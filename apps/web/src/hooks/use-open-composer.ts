import { useCallback } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"
import { searchFromSeed, type LaunchSeed } from "@/lib/launch-seed"

// EXP-825: "start something" is a NAVIGATION now — every play button (issue
// detail, the bulk bar, an action's Run, New action / a suggestion, a
// device's play, Reviews' Fix conflicts) sends the person to the Agent page
// composer with a preselection, the way `useOpenSession` sends them to a
// run. ONE place decides the URL shape.

export function useOpenComposer(): (seed: Partial<LaunchSeed>) => void {
  const navigate = useNavigate()
  // Loose match: every caller lives under `/t/$teamSlug`, but the hook must
  // not throw in a story/test that mounts one outside the team layout.
  const { teamSlug } = useParams({ strict: false })

  return useCallback(
    (seed: Partial<LaunchSeed>) => {
      if (!teamSlug) {
        console.warn(`useOpenComposer: no teamSlug in scope, ignoring`)
        return
      }
      void navigate({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
        search: searchFromSeed(seed),
      })
    },
    [navigate, teamSlug]
  )
}
