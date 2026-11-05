import { useRouterState } from "@tanstack/react-router"
import { sidebarOccupant, type SidebarOccupant } from "@/lib/detail-origin"

// EXP-851 / EXP-870: which panel sits in the sidebar column — derived from
// the URL, never click state, so every entry point drives the same swap and
// a deep link lands settled. Pathname off the router STATE, not
// useMatchRoute: the matches lag the eagerly updated location during a
// pending navigation. Shared by the team layout (the sidebar's width) and the
// sidebar itself (the main menu ⇄ settings nav swap).
export function useSidebarOccupant(): SidebarOccupant {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  return sidebarOccupant(pathname)
}
