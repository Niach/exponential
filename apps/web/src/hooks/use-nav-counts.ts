import { useMemo } from "react"
import { chatsBadge, type CountBadgeValue } from "@exp/domain-contract/nav-badges"
import { useSessionReads, useChatsList } from "@/hooks/use-session-tree"

/**
 * #FEED-232 (nav-badges.json): the Chats destination's count badge
 * (`chatsBadge`) for the phone's Chats tab; md+ reads the sidebar's own
 * list. `teamIds` = the scope (phones: the ONE active team).
 */
export function useChatsBadge(teamIds: string | readonly string[] | undefined): CountBadgeValue {
  const { groups } = useChatsList(teamIds)
  const reads = useSessionReads()
  return useMemo(() => chatsBadge(groups, reads), [groups, reads])
}
