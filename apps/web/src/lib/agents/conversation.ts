import { TRPCError } from "@trpc/server"
import { and, eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, sessions, type Agent, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { resolveSessionDevice } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"

// NEW-2: an agent RUNS as conversation sessions: ONE root per (agent,
// member) = that member's private DM with it, on the agent's device and
// login, never ending (session-kind.json: persistent). Created on first
// use (a message, a trigger firing into the owner's); a message resumes
// the same row. The row mirrors the agent's name/icon/color and `kind =
// agent` for the shipped clients (legacy read shims, NEW-8 drops them).

/** The agent, when the caller is a member of its team. */
export async function loadAgentForUser(userId: string, agentId: string): Promise<Agent> {
  const [agent] = await db.select().from(agents).where(eq(agents.id, agentId)).limit(1)
  if (!agent || !(await getTeamMember(userId, agent.teamId))) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  }
  return agent
}

/** The conversation session a member has with an agent, if it exists. */
export async function findConversation(
  tx: Tx | typeof db,
  agentId: string,
  userId: string
): Promise<Session | null> {
  const [row] = await tx
    .select()
    .from(sessions)
    .where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId), isNull(sessions.parentId)))
    .limit(1)
  return row ?? null
}

/** The legacy mirror columns a conversation session carries for the
 *  shipped clients (re-stamped by `agents.update`). */
export function agentMirror(agent: Pick<Agent, `name` | `icon` | `color`>) {
  return { kind: `agent` as const, name: agent.name, icon: agent.icon, color: agent.color }
}

/**
 * The member's conversation session with the agent, created `queued` on the
 * agent's device when it does not exist yet. The device must be the
 * member's own or shared with the team (`resolveSessionDevice`): an agent
 * on an unshared device talks to its owner only. Returns the row and, for a
 * fresh one, the host to nudge.
 */
export async function conversationInTx(
  tx: Tx,
  agent: Agent,
  userId: string
): Promise<{ session: Session; created: { ownerId: string; deviceId: string } | null }> {
  const existing = await findConversation(tx, agent.id, userId)
  if (existing) return { session: existing, created: null }
  const device = await resolveSessionDevice(userId, agent.teamId, agent.deviceId)
  const id = crypto.randomUUID()
  const [session] = await tx
    .insert(sessions)
    .values({
      id,
      teamId: agent.teamId,
      parentId: null,
      rootId: id,
      depth: 0,
      agentId: agent.id,
      ...agentMirror(agent),
      userId,
      deviceId: agent.deviceId,
      cli: agent.cli,
      account: agent.account,
      model: agent.model,
      repositoryId: null,
      status: `queued`,
      shared: device.shared,
      title: agent.name,
    })
    .onConflictDoNothing()
    .returning()
  if (!session) {
    // Two first messages raced: the other one's row is the conversation.
    const raced = await findConversation(tx, agent.id, userId)
    if (!raced) throw new TRPCError({ code: `CONFLICT`, message: `The conversation just changed; try again.` })
    return { session: raced, created: null }
  }
  return { session, created: { ownerId: device.ownerId, deviceId: agent.deviceId } }
}
