import { useRouterState } from "@tanstack/react-router"
import {
  secondSidebarFor,
  sidebarOccupant,
  type DetailOrigin,
  type SidebarOccupant,
} from "@/lib/detail-origin"

// EXP-851 / EXP-870: which panel sits in the sidebar column — derived from
// the URL, never click state, so every entry point drives the same swap and
// a deep link lands settled. Pathname off the router STATE, not
// useMatchRoute: the matches lag the eagerly updated location during a
// pending navigation. Shared by the team layout (the sidebar's width) and the
// sidebar itself (the rail's compact state and the settings panel).
export function useSidebarOccupant(): SidebarOccupant {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  return sidebarOccupant(pathname)
}

// #EXP-1672: the content card's second sidebar (the Inbox list) — from the
// pathname + `?from=` + the Inbox page's `?tab=`.
export function useSecondSidebar(): DetailOrigin | null {
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
  return secondSidebarFor(pathname, fromToken, { tab: tabToken })
}
