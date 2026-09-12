import { useCallback } from "react"
import {
  useLocation,
  useNavigate,
  useParams,
  useSearch,
} from "@tanstack/react-router"
import { searchFromSeed, type LaunchSeed } from "@/lib/launch-seed"
import {
  capturedOrigin,
  formatOrigin,
  parseOrigin,
  screenFromPath,
} from "@/lib/detail-origin"

// EXP-825: "start something" is a NAVIGATION now — every play button (issue
// detail, the bulk bar, an action's Run, New action / a suggestion, a
// device's play, Reviews' Fix conflicts) sends the person to the Agent page
// composer with a preselection, the way `useOpenSession` sends them to a
// run. ONE place decides the URL shape.
//
// EXP-851: the composer also carries the ORIGIN the click came from
// (`?from=`), so the Agent page keeps that list nav beside it and the run it
// launches inherits it — starting from an issue detail lands the run on that
// issue's own session route.

export function useOpenComposer(): (seed: Partial<LaunchSeed>) => void {
  const navigate = useNavigate()
  // Loose match: every caller lives under `/t/$teamSlug`, but the hook must
  // not throw in a story/test that mounts one outside the team layout.
  const { teamSlug } = useParams({ strict: false })
  const location = useLocation({ select: (current) => current.pathname })
  const from = useSearch({
    strict: false,
    select: (search) =>
      typeof (search as { from?: unknown }).from === `string`
        ? ((search as { from?: string }).from ?? null)
        : null,
  })

  return useCallback(
    (seed: Partial<LaunchSeed>) => {
      if (!teamSlug) {
        console.warn(`useOpenComposer: no teamSlug in scope, ignoring`)
        return
      }
      const screen = screenFromPath(location ?? ``)
      const token = formatOrigin(capturedOrigin(screen, parseOrigin(from)))
      void navigate({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
        search: { ...searchFromSeed(seed), ...(token ? { from: token } : {}) },
      })
    },
    [navigate, teamSlug, location, from]
  )
}
