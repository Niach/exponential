// #EXP-2161 (member-space.json `pinCases`): which roots are the member's OWN
// Assistant conversation, off the synced rows: the one tree a member space's
// install reads, so the only one its pin counts for. The earlier conversation
// (detached by a pick, `agent_id` null) never is.

import type { RootSight } from "@exp/domain-contract/member-space"
import { agentCollection, sessionCollection } from "@/lib/collections"

type AgentLike = { role?: string | null; ownerId?: string | null }
type SessionLike = { parentId: string | null; userId: string; agentId?: string | null }

export const isOwnAssistantAgent = (agent: AgentLike | undefined, me: string): boolean =>
  agent?.role === `assistant` && agent.ownerId === me

export const isOwnAssistantRoot = (root: SessionLike | undefined, agent: AgentLike | undefined, me: string): boolean =>
  root !== undefined && root.parentId === null && root.userId === me && isOwnAssistantAgent(agent, me)

/** What the synced rows say of a root (`rootMarkCases`): a row or an agent not
 *  synced yet says nothing. */
export function sightOfRoot(root: SessionLike | undefined, agent: AgentLike | undefined, me: string): RootSight {
  if (!root) return `unknown`
  if (root.agentId && !agent && root.parentId === null && root.userId === me) return `unknown`
  return isOwnAssistantRoot(root, agent, me) ? `assistant` : `other`
}

export const ownAssistantAgent = (agentId: string, me: string): boolean =>
  isOwnAssistantAgent(agentCollection.get(agentId) as AgentLike | undefined, me)

export function rootSight(rootId: string, me: string): RootSight {
  const root = sessionCollection.get(rootId) as SessionLike | undefined
  const agent = root?.agentId ? (agentCollection.get(root.agentId) as AgentLike | undefined) : undefined
  return sightOfRoot(root, agent, me)
}
