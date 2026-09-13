import { useRouterState } from "@tanstack/react-router"
import { sidebarOccupant, type SidebarOccupant } from "@/lib/detail-origin"

// EXP-851 / EXP-870: which panel sits beside the rail — derived from the URL
// alone (pathname + `?from=`), never click state, so every entry point drives
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
  return sidebarOccupant(pathname, fromToken)
}
