import { useRouterState } from "@tanstack/react-router"
import { sidebarOccupant, type SidebarOccupant } from "@/lib/detail-origin"
import { useRecentRunsPanelOpen } from "@/lib/recent-runs-panel"

// EXP-851 / EXP-870: which panel sits beside the rail — derived from the URL
// (pathname + `?from=` + the Inbox page's `?tab=`), never click state, so
// every entry point drives the same swap and a deep link lands settled.
// Pathname off the router STATE, not useMatchRoute: the matches lag the
// eagerly updated location during a pending navigation. Shared by the team
// layout (the sidebar's width) and the sidebar itself (the rail's compact
// state and the sliding panel).
//
// EXP-1246: the one input that is not the URL is the Agent page's Recent
// panel flag (`lib/recent-runs-panel.ts`), and it only counts ON that page —
// a run opened from Recent carries `?from=agent:recent` instead.
export function useSidebarOccupant(): SidebarOccupant {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const fromToken = useRouterState({
    select: (s) => {
      const value = (s.location.search as { from?: unknown }).from
      return typeof value === `string` ? value : null
    },
  })
  const tabToken = useRouterState({
    select: (s) => {
      const value = (s.location.search as { tab?: unknown }).tab
      return typeof value === `string` ? value : null
    },
  })
  const recentOpen = useRecentRunsPanelOpen()
  return sidebarOccupant(pathname, fromToken, { tab: tabToken, recentOpen })
}
