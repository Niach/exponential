import { agentCollection } from "@/lib/collections"

// #EXP-1990 (agent-settings.json `groupChat`): a team agent's ONE chat is
// its group chat, the root `agents.groupChatId` names: every member's chat
// with the agent, whoever started it.

/** `root` is its agent's group chat (read off the synced agents). */
export function isGroupChatRoot(root: { id: string; agentId?: string | null }): boolean {
  if (!root.agentId) return false
  return (agentCollection.get(root.agentId) as { groupChatId?: string | null } | undefined)?.groupChatId === root.id
}
