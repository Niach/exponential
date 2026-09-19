import { useRouterState } from "@tanstack/react-router"
import { sidebarOccupant, type SidebarOccupant } from "@/lib/detail-origin"
import { useReviewFilesSubjectId } from "@/lib/review-files-slot"
import { useRecentRunsPanelOpen } from "@/lib/recent-runs-panel"

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
  // the panel would sit on "Loading changes" for good. Only the published
  // SUBJECT is read (a primitive), never the slot: the slot republishes a
  // fresh object on every diff tick and file pick, and this hook sits under
  // the whole team layout. It must be THIS run's tree — a slot left over
  // from the previous review would otherwise swap the panel in for a frame.
  const subjectId = useReviewFilesSubjectId()
  const sessionId = /\/sessions\/([^/]+)$/.exec(pathname)?.[1] ?? null
  // EXP-923: the ONE occupant the URL does not decide — the Agent page's
  // Recent panel, opened by that page's history button and dropped when the
  // page unmounts (`lib/recent-runs-panel.ts`). It can only be up while that
  // route is, so a deep link still lands settled.
  const recentOpen = useRecentRunsPanelOpen()
  if (recentOpen && /\/agent$/.test(pathname)) return { kind: `recent` }
  return sidebarOccupant(
    pathname,
    fromToken,
    sessionId !== null && subjectId !== sessionId ? null : viewToken
  )
}
