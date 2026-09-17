import { useRouterState } from "@tanstack/react-router"
import { sidebarOccupant, type SidebarOccupant } from "@/lib/detail-origin"
import { useReviewFilesSlot } from "@/lib/review-files-slot"

// EXP-851 / EXP-870: which panel sits beside the rail — derived from the URL
// alone (pathname + `?from=` + `?view=`), never click state, so every entry point drives
// the same swap and a deep link lands settled. Pathname off the router STATE,
// not useMatchRoute: the matches lag the eagerly updated location during a
// pending navigation. Shared by the team layout (the sidebar's width) and the
// sidebar itself (the rail's compact state and the sliding panel).
export function useSidebarOccupant(): SidebarOccupant {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const fromToken = useRouterState({
    select: (s) => {
      const value = (s.location.search as { from?: unknown }).from
      return typeof value === `string` ? value : null
    },
  })
  // EXP-945: the run's Changes face (`?view=diff`) puts its file tree in the
  // panel, exactly where a review's sits.
  const viewToken = useRouterState({
    select: (s) => {
      const value = (s.location.search as { view?: unknown }).view
      return typeof value === `string` ? value : null
    },
  })
  // A run publishes its tree only while its Changes face really draws (the
  // owner's live view). A teammate's read-only run, or a run still loading,
  // has nothing to put in the panel, so the URL alone must not swap it in or
  // the panel would sit on "Loading changes" for good.
  const slot = useReviewFilesSlot()
  const onRun = /\/sessions\/[^/]+$/.test(pathname)
  return sidebarOccupant(
    pathname,
    fromToken,
    onRun && slot === null ? null : viewToken
  )
}
