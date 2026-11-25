// #EXP-2161 (member-space.json `pinCases`): which roots are the member's OWN
// Assistant conversation, off the synced rows: the one tree a member space's
// install reads, so the only one its pin counts for. The earlier conversation
// (detached by a pick, `agent_id` null) never is.

import { agentCollection, sessionCollection } from "@/lib/collections"

type AgentLike = { role?: string | null; ownerId?: string | null }
type SessionLike = { parentId: string | null; userId: string; agentId?: string | null }

export const isOwnAssistantAgent = (agent: AgentLike | undefined, me: string): boolean =>
  agent?.role === `assistant` && agent.ownerId === me

export const isOwnAssistantRoot = (root: SessionLike | undefined, agent: AgentLike | undefined, me: string): boolean =>
  root !== undefined && root.parentId === null && root.userId === me && isOwnAssistantAgent(agent, me)

export const ownAssistantAgent = (agentId: string, me: string): boolean =>
  isOwnAssistantAgent(agentCollection.get(agentId) as AgentLike | undefined, me)

export function ownAssistantRoot(rootId: string, me: string): boolean {
  const root = sessionCollection.get(rootId) as SessionLike | undefined
  const agent = root?.agentId ? (agentCollection.get(root.agentId) as AgentLike | undefined) : undefined
  return isOwnAssistantRoot(root, agent, me)
}
