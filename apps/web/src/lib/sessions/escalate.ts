import { TRPCError } from "@trpc/server"
import { and, eq, isNotNull, isNull, ne, sql } from "drizzle-orm"
import type { MessageCard } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { messages, sessions, type Session } from "@/db/schema"
import { askText } from "@/lib/sessions/bus"
import { insertMessage } from "@/lib/sessions/messages"
import { loadSessionForUser } from "@/lib/sessions/access"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1399: ESCALATE a waiting question / plan card into the team agent's
// conversation that manages its thread. The escalated card is an `ask`
// mirror ({sessionId, messageId}, like the bus's), so every client draws
// THE origin card there (session-thread.json): one row, answered through
// `sessions.answer(origin)` with the origin's own auth, first answer wins,
// and `closeAskMirrors` closes the mirror with it. A root's own cards reach
// the conversation that started it already (it watches the root); this op
// lifts a card from anywhere in that tree.

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

type AskCard = Extract<MessageCard, { kind: `question` | `plan` }>

/** The agent conversation managing `root`'s tree: a live agent
 *  conversation watching the root, else the root's starting agent's
 *  conversation with the root's starter. */
export async function managingConversation(
  tx: Tx,
  root: Pick<Session, `id` | `teamId` | `userId` | `startedByAgentId`>
): Promise<Session | null> {
  const [watching] = await tx
    .select()
    .from(sessions)
    .where(
      and(
        eq(sessions.teamId, root.teamId),
        isNotNull(sessions.agentId),
        isNull(sessions.parentId),
        ne(sessions.status, `ended`),
        sql`${sessions.wakeOn} @> ${JSON.stringify([root.id])}::jsonb`
      )
    )
    .orderBy(sessions.id)
    .limit(1)
  if (watching) return watching
  if (!root.startedByAgentId) return null
  const [conversation] = await tx
    .select()
    .from(sessions)
    .where(
      and(
        eq(sessions.agentId, root.startedByAgentId),
        eq(sessions.userId, root.userId),
        isNull(sessions.parentId)
      )
    )
    .limit(1)
  return conversation ?? null
}

export async function escalateCard(input: {
  userId: string
  /** The calling session (in the card's tree, or the managing conversation). */
  caller: Pick<Session, `id` | `rootId`>
  messageId: string
}): Promise<{ messageId: string; sessionId: string; escalated: boolean }> {
  let [message] = await db.select().from(messages).where(eq(messages.id, input.messageId)).limit(1)
  // An `ask` mirror (the bus's, in a parent) escalates its origin card.
  const mirrored = message?.card as { kind?: unknown; messageId?: unknown } | null | undefined
  if (message && mirrored?.kind === `ask` && typeof mirrored.messageId === `string`) {
    ;[message] = await db.select().from(messages).where(eq(messages.id, mirrored.messageId)).limit(1)
  }
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const card = message.card as MessageCard | null
  if (card?.kind !== `question` && card?.kind !== `plan`) {
    throw bad(`messageId must name a question or plan card (or its ask mirror)`)
  }
  if (message.answeredAt) throw bad(`That card is already answered`)
  const origin = await loadSessionForUser(input.userId, message.sessionId)
  const [root] = await db.select().from(sessions).where(eq(sessions.id, origin.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

  return db.transaction(async (tx) => {
    const target = await managingConversation(tx, root)
    if (!target) {
      throw bad(`No team agent manages this thread: escalate works in a thread an agent's conversation started or watches`)
    }
    if (input.caller.rootId !== root.id && input.caller.rootId !== target.id) {
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `Escalate a card of your own thread, or one of a thread your conversation manages`,
      })
    }
    if (target.id === root.id) throw bad(`That card is already in the agent's conversation`)
    const [existing] = await tx
      .select({ id: messages.id })
      .from(messages)
      .where(
        and(
          eq(messages.sessionId, target.id),
          eq(messages.authorSessionId, origin.id),
          sql`${messages.card}->>'kind' = 'ask'`,
          sql`${messages.card}->>'messageId' = ${message.id}`
        )
      )
      .limit(1)
    if (existing) return { messageId: existing.id, sessionId: target.id, escalated: false }
    const id = await insertMessage(tx, target, {
      authorKind: `agent`,
      authorSessionId: origin.id,
      body: askText(origin, card as AskCard),
      card: { kind: `ask`, sessionId: origin.id, messageId: message.id },
      // It wakes the agent, unless the agent escalated it itself.
      consumedAt: input.caller.id === target.id || target.status === `ended` ? new Date() : null,
    })
    return { messageId: id, sessionId: target.id, escalated: true }
  })
}
