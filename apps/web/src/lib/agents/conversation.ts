import { TRPCError } from "@trpc/server"
import { and, eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, apps, devices, sessions, type Agent, type Session } from "@/db/schema"
import { appRootDevice, createEditorSessionInTx, defaultDeviceFor } from "@/lib/apps/editor-session"
import { getTeamMember } from "@/lib/auth/membership"
import { sessionVisibleTo } from "@/lib/sessions/access"
import { SHARING_ORDER } from "@/lib/sessions/host"
import { DEVICE_NOT_SHARED_MESSAGE, resolveSessionDevice } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"
import { endPrivateConversationsInTx, groupChatAgent, restampTreesInTx } from "./group-chat"

// NEW-2: an agent RUNS as conversation sessions, on the agent's device and
// login, never ending (session-kind.json: persistent). Created on first
// use (a message, a trigger firing); a message resumes the same row. Name,
// icon and color live on the agent only. An Assistant: ONE root per (agent,
// member), its member's private chat. #EXP-1990 (agent-settings.json
// `groupChat`): a team agent, once `AGENT_GROUP_CHATS` is on: ONE root for
// the whole team, its GROUP CHAT (`agents.group_chat_id`); off, one root
// per (agent, member) as before. An app's agent keeps its editor root per
// (app, member).

/** The agent, when the caller is a member of its team (an Assistant: its owner). */
export async function loadAgentForUser(userId: string, agentId: string): Promise<Agent> {
  const [agent] = await db.select().from(agents).where(eq(agents.id, agentId)).limit(1)
  // S-3: another member's Assistant is theirs alone (agent-settings.json
  // `assistant`): never read, messaged or edited by anyone else.
  if (!agent || (agent.role === `assistant` && agent.ownerId !== userId) || !(await getTeamMember(userId, agent.teamId))) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  }
  return agent
}

type ConversationAgent = Pick<Agent, `id` | `role` | `groupChatId`>

/** The session `userId` talks to the agent in, if it exists: the agent's
 *  group chat (#EXP-1990; whoever started it, while they may see it), else
 *  their own conversation with it. */
export async function findConversation(
  tx: Tx | typeof db,
  agent: ConversationAgent,
  userId: string
): Promise<Session | null> {
  const group = groupChatAgent(agent)
  if (group && agent.groupChatId) {
    const [chat] = await tx
      .select()
      .from(sessions)
      .where(and(eq(sessions.id, agent.groupChatId), isNull(sessions.parentId)))
      .limit(1)
    // An agent on an unshared machine talks to its owner only.
    if (chat) return sessionVisibleTo(chat, userId) ? chat : null
  }
  const [row] = await tx
    .select()
    .from(sessions)
    .where(and(eq(sessions.agentId, agent.id), eq(sessions.userId, userId), isNull(sessions.parentId)))
    .limit(1)
  // A team agent's own-conversation rows are only an app's editor roots now
  // (a private conversation the move has not taken yet is never resumed).
  if (group && row && !row.appId) return null
  return row ?? null
}

/** Whether the machine a group chat runs on is shared with its team, read
 *  off its HOST's devices row (`resolveSessionHost`: the starter's own row,
 *  else the first sharing row); null = no row names the machine (unknown:
 *  the stamp stays). */
async function hostShares(tx: Tx, chat: Pick<Session, `deviceId` | `userId` | `teamId`>): Promise<boolean | null> {
  const rows = await tx
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, chat.deviceId))
    .orderBy(...SHARING_ORDER)
  const shares = (row: (typeof rows)[number]) => (row.sharedTeamIds ?? []).includes(chat.teamId)
  const own = rows.find((row) => row.userId === chat.userId)
  if (own) return shares(own)
  return rows.length === 0 ? null : rows.some(shares)
}

/** #EXP-1990: the agent's group chat for `userId`, under the agent's row
 *  lock: found (re-stamped when its machine's share changed; a member who
 *  may not see it is refused as for the machine) or created. */
async function groupChatInTx(
  tx: Tx,
  agent: Agent,
  userId: string
): Promise<{ session: Session; created: { ownerId: string; deviceId: string } | null }> {
  const [locked] = await tx.select().from(agents).where(eq(agents.id, agent.id)).for(`update`)
  if (!locked) throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  const [existing] = locked.groupChatId
    ? await tx.select().from(sessions).where(and(eq(sessions.id, locked.groupChatId), isNull(sessions.parentId))).limit(1)
    : []
  if (existing) {
    // The root follows its machine: shared with the team = everyone's.
    const shared = await hostShares(tx, existing)
    const session = shared === null || shared === existing.shared ? existing : { ...existing, shared }
    if (session !== existing) await restampTreesInTx(tx, [existing.id], shared!)
    // An agent on an unshared machine talks to its owner only.
    if (!sessionVisibleTo(session, userId)) throw new TRPCError({ code: `NOT_FOUND`, message: DEVICE_NOT_SHARED_MESSAGE })
    return { session, created: null }
  }
  const device = await resolveSessionDevice(userId, locked.teamId, locked.deviceId, tx)
  // Its starter: the agent's owner while they are a member who may run on
  // its machine (their triggers fire into it), else the member opening it.
  const ownerRuns =
    locked.ownerId !== userId &&
    !!(await getTeamMember(locked.ownerId, locked.teamId)) &&
    (await resolveSessionDevice(locked.ownerId, locked.teamId, locked.deviceId, tx).then(
      () => true,
      () => false
    ))
  const starterId = ownerRuns ? locked.ownerId : userId
  // A conversation the boot move has not taken yet (the switch just went
  // on) ends now: one chat per team agent.
  await endPrivateConversationsInTx(tx, locked.id)
  const id = crypto.randomUUID()
  const [session] = await tx
    .insert(sessions)
    .values({ ...conversationValues(locked, id), userId: starterId, shared: device.shared })
    .returning()
  await tx.update(agents).set({ groupChatId: id }).where(eq(agents.id, locked.id))
  return { session: session!, created: { ownerId: device.ownerId, deviceId: locked.deviceId } }
}

/** A fresh conversation root's values, but its starter and visibility. */
function conversationValues(agent: Agent, id: string) {
  return {
    id,
    teamId: agent.teamId,
    parentId: null,
    rootId: id,
    depth: 0,
    agentId: agent.id,
    deviceId: agent.deviceId,
    cli: agent.cli,
    account: agent.account,
    model: agent.model,
    // #EXP-1656: the agent's skills (its triggers fire into this row too).
    skillIds: agent.skillIds.length > 0 ? agent.skillIds : null,
    // #EXP-1793: and its MCP servers.
    mcpServerIds: agent.mcpServerIds.length > 0 ? agent.mcpServerIds : null,
    repositoryId: null,
    status: `queued` as const,
    title: agent.name,
  }
}

/**
 * The session `userId` talks to the agent in, created `queued` on the
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
  // #EXP-1849: an app's agent talks to a member in the app's root (the
  // editor), on its slot or the member's default device.
  const [app] = await tx.select().from(apps).where(and(eq(apps.agentId, agent.id), isNull(apps.deletedAt))).limit(1)
  if (app) {
    const existing = await findConversation(tx, agent, userId)
    if (existing) return { session: existing, created: null }
    const picked = appRootDevice(agent, await defaultDeviceFor(userId))
    if (!picked) throw new TRPCError({ code: `BAD_REQUEST`, message: `Start a device to talk to this app's agent` })
    return createEditorSessionInTx(tx, app, userId, picked, {})
  }
  if (groupChatAgent(agent)) return groupChatInTx(tx, agent, userId)
  const existing = await findConversation(tx, agent, userId)
  if (existing) return { session: existing, created: null }
  const device = await resolveSessionDevice(userId, agent.teamId, agent.deviceId)
  const id = crypto.randomUUID()
  const [session] = await tx
    .insert(sessions)
    .values({ ...conversationValues(agent, id), userId, shared: device.shared })
    .onConflictDoNothing()
    .returning()
  if (!session) {
    // Two first messages raced: the other one's row is the conversation.
    const raced = await findConversation(tx, agent, userId)
    if (!raced) throw new TRPCError({ code: `CONFLICT`, message: `The conversation just changed; try again.` })
    return { session: raced, created: null }
  }
  return { session, created: { ownerId: device.ownerId, deviceId: agent.deviceId } }
}
