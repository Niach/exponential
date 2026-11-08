import { useMemo } from "react"
import { useSessionReads, useChatsList } from "@/hooks/use-session-tree"
import { chatsNavState } from "@/components/team/sidebar-running"

// Shared nav-count hooks for the sidebar badges (desktop) and the mobile
// tab bar dots (EXP-1327: the live dot rides Chats only, ×4). Both count purely client-side over already-synced shapes.

/**
 * EXP-1293: the Chats entry's dot (the phone's Chats tab; md+ reads the
 * sidebar's own list) — green while any chat of the scope runs, amber
 * while one needs you. `teamIds` = the scope (the phone: every member team).
 */
export function useChatsNav(teamIds: string | readonly string[] | undefined): {
  live: boolean
  needsYou: boolean
  /** EXP-1323: a chat holds an agent reply the viewer has not read. */
  unread: boolean
} {
  const { groups } = useChatsList(teamIds)
  const reads = useSessionReads()
  return useMemo(() => chatsNavState(groups, reads), [groups, reads])
}
