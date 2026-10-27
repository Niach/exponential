import { useMemo } from "react"
import { useSessionReads, useThreadsList } from "@/hooks/use-session-tree"
import { threadsNavState } from "@/components/team/sidebar-running"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots (EXP-1327: the live dot rides Threads only, ×4). Both count purely client-side over already-synced shapes.

/**
 * EXP-1293: the Threads entry's dot (the phone's Threads tab; md+ reads the
 * sidebar's own list) — green while any thread of the scope runs, amber
 * while one needs you. `teamIds` = the scope (the phone: every member team).
 */
export function useThreadsNav(teamIds: string | readonly string[] | undefined): {
  live: boolean
  needsYou: boolean
  /** EXP-1323: a thread holds an agent reply the viewer has not read. */
  unread: boolean
} {
  const { groups } = useThreadsList(teamIds)
  const reads = useSessionReads()
  return useMemo(() => threadsNavState(groups, reads), [groups, reads])
}
