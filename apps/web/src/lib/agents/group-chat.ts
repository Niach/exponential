import { and, desc, eq, inArray, isNotNull, isNull, ne, or, sql } from "drizzle-orm"
import { groupAskRecipient } from "@exp/domain-contract/agent-settings"
import { db } from "@/db/connection"
import { closeAskMirrorsOf } from "@/lib/sessions/bus"
import { agents, conversations, messages, sessionIssues, sessions, teamMembers, type Agent } from "@/db/schema"
import { inSession, pendingInput } from "@/lib/sessions/messages"
import { hidePrivateLinesInTx } from "@/lib/sessions/private-lines"
import { dropNonReaderKeysInTx } from "@/lib/sessions/tree-keys"
import type { Tx } from "@/lib/sessions/tx"
import { agentGroupChatsOn } from "./group-chat-switch"

// #EXP-1990 (agent-settings.json `groupChat`, phase 6 of #EXP-1984): a team
// agent talks in ONE group chat, the root `agents.group_chat_id` names,
// behind ONE switch. Off (unset, the default): one conversation per (agent,
// member) as before, `group_chat_id` stays null, nothing moves. On
// (`AGENT_GROUP_CHATS=on`, set by the release shepherd only once the clients
// that open the chat by id are released and the minimum versions name them):
// the first use of a team agent creates its group chat
// (lib/agents/conversation.ts) and every boot ends the private
// conversations still attached to a team agent (idempotent).

export { agentGroupChatsOn } from "./group-chat-switch"

/** The agent talks in a group chat: a team agent (never an Assistant; an
 *  app's agent is told apart by its `apps` row, where the caller has it)
 *  while the switch is on. */
export function groupChatAgent(agent: Pick<Agent, `role`>): boolean {
  return agentGroupChatsOn() && agent.role !== `assistant`
}

/** `session` is a team agent's group chat: the ROOT its agent's
 *  `group_chat_id` names. */
export async function isGroupChat(
  database: Pick<typeof db, `select`>,
  session: { id: string; parentId: string | null; agentId: string | null }
): Promise<boolean> {
  if (session.parentId !== null || !session.agentId) return false
  const [agent] = await database
    .select({ groupChatId: agents.groupChatId })
    .from(agents)
    .where(eq(agents.id, session.agentId))
    .limit(1)
  return agent?.groupChatId === session.id
}

/**
 * Re-stamps whole trees' visibility (`shared` and its mirrors on messages,
 * conversations and issue links). A tree turning shared: what a private
 * chat's bus wrote in full goes neutral for its new readers (#EXP-1409). A
 * tree turning private keeps only its readers' key wraps (#FEED-177);
 */
export async function restampTreesInTx(tx: Tx, rootIds: string[], shared: boolean): Promise<void> {
  if (rootIds.length === 0) return
  const tree = tx.select({ id: sessions.id }).from(sessions).where(inArray(sessions.rootId, rootIds))
  await tx
    .update(sessions)
    .set({ shared, updatedAt: new Date() })
    .where(and(inArray(sessions.rootId, rootIds), ne(sessions.shared, shared)))
  await tx.update(messages).set({ shared }).where(and(inArray(messages.rootId, rootIds), ne(messages.shared, shared)))
  await tx.update(conversations).set({ shared }).where(inArray(conversations.sessionId, tree))
  await tx.update(sessionIssues).set({ shared }).where(inArray(sessionIssues.sessionId, tree))
  await hidePrivateLinesInTx(tx, rootIds)
  if (!shared) await dropNonReaderKeysInTx(tx, rootIds)
}

/**
 * THE MOVE of one pass (`groupChatMoveOf`, `groupChat.moveCases`): every
 * conversation root of a team agent that is not its agent's group chat
 * (never an Assistant's, never an app's editor root; `agentId` = only that
 * agent's) ENDS with its tree, is detached from the agent and turns
 * private, so it stays readable only by the member who had it. Returns the
 * roots it took.
 */
export async function endPrivateConversationsInTx(tx: Tx, agentId?: string): Promise<string[]> {
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .innerJoin(agents, eq(agents.id, sessions.agentId))
    .where(
      and(
        isNull(sessions.parentId),
        isNull(sessions.appId),
        ne(agents.role, `assistant`),
        or(isNull(agents.groupChatId), ne(agents.groupChatId, sessions.id)),
        agentId ? eq(agents.id, agentId) : undefined
      )
    )
    .orderBy(sessions.id)
  const rootIds = rows.map((row) => row.id)
  if (rootIds.length === 0) return rootIds
  const now = new Date()
  const ended = await tx
    .update(sessions)
    .set({
      status: `ended`,
      endedAt: sql`coalesce(${sessions.endedAt}, ${now})`,
      busy: false,
      background: false,
      needsInput: false,
      blocked: null,
      updatedAt: now,
    })
    .where(and(inArray(sessions.rootId, rootIds), ne(sessions.status, `ended`)))
    .returning({ id: sessions.id })
  // #FEED-299: an ended run's waiting cards (and their mirrors) close with it.
  await closeAskMirrorsOf(tx, ended.map((row) => row.id), now)
  // Detached: no cascade takes the tree with its agent, and it reads as an
  // ended chat (titled after the agent, as every conversation is).
  await tx.update(sessions).set({ agentId: null }).where(inArray(sessions.rootId, rootIds))
  // #EXP-2119: due for a rotation (its member may write on): a holder rotates
  // it, the old epoch stays readable to the member (OLDER EPOCHS).
  await restampTreesInTx(tx, rootIds, false)
  return rootIds
}

/**
 * Who is pushed a question or plan `session` asks (`groupChat.askCases`),
 * when it is a team agent's group chat; null = it is none (the usual rule:
 * its starter). The asker = the author of the newest person message among
 * the inputs the device delivered LAST (one `consumed_at`); a turn a
 * trigger, an event or a child's line started has none: the agent's owner.
 */
export async function groupAskRecipients(
  database: Pick<typeof db, `select`>,
  session: { id: string; parentId: string | null; agentId: string | null; teamId: string }
): Promise<string[] | null> {
  if (session.parentId !== null || !session.agentId) return null
  const [agent] = await database
    .select({ ownerId: agents.ownerId, groupChatId: agents.groupChatId })
    .from(agents)
    .where(eq(agents.id, session.agentId))
    .limit(1)
  if (!agent || agent.groupChatId !== session.id) return null
  const [delivered] = await database
    .select({ at: messages.consumedAt })
    .from(messages)
    .where(and(inSession(session.id), pendingInput(), isNotNull(messages.consumedAt)))
    .orderBy(desc(messages.consumedAt))
    .limit(1)
  const [asked] = delivered?.at
    ? await database
        .select({ authorId: messages.authorId })
        .from(messages)
        .where(
          and(
            inSession(session.id),
            eq(messages.consumedAt, delivered.at),
            eq(messages.authorKind, `user`),
            isNull(messages.authorSessionId),
            isNotNull(messages.authorId),
            or(isNull(messages.card), sql`${messages.card}->>'kind' <> 'event'`)
          )
        )
        .orderBy(desc(messages.createdAt))
        .limit(1)
    : []
  const members = await database
    .select({ userId: teamMembers.userId })
    .from(teamMembers)
    .where(eq(teamMembers.teamId, session.teamId))
  return groupAskRecipient({
    turnStarterId: asked?.authorId ?? null,
    ownerId: agent.ownerId,
    memberIds: members.map((member) => member.userId),
  })
}

type Runner = Pick<typeof db, `transaction`>

/** Runs the move in ONE transaction under an advisory lock (two instances
 *  booting at once queue; the second finds nobody left). No-op while off. */
export async function runGroupChatMove(database: Runner = db): Promise<string[]> {
  if (!agentGroupChatsOn()) return []
  return database.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext('exp1990-group-chat-move'))`)
    return endPrivateConversationsInTx(tx)
  })
}
