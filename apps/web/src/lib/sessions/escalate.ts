import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, isNotNull, isNull, ne, sql } from "drizzle-orm"
import type { MessageCard } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { messages, sessions, type Session } from "@/db/schema"
import { askHidden, askHiddenText, askText } from "@/lib/sessions/bus"
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

/** The agent conversation managing `origin`'s tree (`root`): a live
 *  agent conversation watching the root, the starter's own first, else
 *  the root's starting agent's live conversation with the root's starter.
 *  #EXP-1399 review: never one whose readers may not see `origin` (its
 *  account must see it; a shared conversation only for a shared origin). */
export async function managingConversation(
  tx: Tx,
  root: Pick<Session, `id` | `teamId` | `userId` | `startedByAgentId`>,
  origin: Pick<Session, `shared` | `userId`>
): Promise<Session | null> {
  const readable = origin.shared ? undefined : and(eq(sessions.userId, origin.userId), eq(sessions.shared, false))
  const conversation = and(
    eq(sessions.teamId, root.teamId),
    isNotNull(sessions.agentId),
    isNull(sessions.parentId),
    ne(sessions.status, `ended`),
    readable
  )
  const [watching] = await tx
    .select()
    .from(sessions)
    .where(and(conversation, sql`${sessions.wakeOn} @> ${JSON.stringify([root.id])}::jsonb`))
    .orderBy(
      desc(sql`${sessions.userId} = ${root.userId}`),
      desc(sql`${sessions.agentId} IS NOT DISTINCT FROM ${root.startedByAgentId}`),
      asc(sessions.id)
    )
    .limit(1)
  if (watching) return watching
  if (!root.startedByAgentId) return null
  const [started] = await tx
    .select()
    .from(sessions)
    .where(and(conversation, eq(sessions.agentId, root.startedByAgentId), eq(sessions.userId, root.userId)))
    .orderBy(desc(sessions.createdAt), desc(sessions.id))
    .limit(1)
  return started ?? null
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
  const origin = await loadSessionForUser(input.userId, message.sessionId)
  const [root] = await db.select().from(sessions).where(eq(sessions.id, origin.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

  return db.transaction(async (tx) => {
    // The card row is THE lock: an answer racing this either lands first
    // (refused below) or waits and then closes the mirror inserted here;
    // a second escalate waits and finds the first one's mirror.
    const [locked] = await tx.select().from(messages).where(eq(messages.id, message.id)).limit(1).for(`update`)
    const card = locked?.card as MessageCard | null | undefined
    if (card?.kind !== `question` && card?.kind !== `plan`) {
      throw bad(`messageId must name a question or plan card (or its ask mirror)`)
    }
    if (locked!.answeredAt) throw bad(`That card is already answered`)
    const target = await managingConversation(tx, root, origin)
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
          sql`${messages.card}->>'kind' = 'ask'`,
          sql`${messages.card}->>'messageId' = ${message.id}`
        )
      )
      .limit(1)
    if (existing) return { messageId: existing.id, sessionId: target.id, escalated: false }
    const id = await insertMessage(tx, target, {
      authorKind: `agent`,
      authorSessionId: origin.id,
      body: askHidden(target, origin) ? askHiddenText(origin, card) : askText(origin, card),
      card: { kind: `ask`, sessionId: origin.id, messageId: message.id },
      // It wakes the agent, unless the agent escalated it itself.
      consumedAt: input.caller.id === target.id || target.status === `ended` ? new Date() : null,
    })
    return { messageId: id, sessionId: target.id, escalated: true }
  })
}
