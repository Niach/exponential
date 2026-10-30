import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, isNotNull, isNull, ne, sql } from "drizzle-orm"
import type { MessageCard } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { messages, sessions, type Session } from "@/db/schema"
import { askHidden, askHiddenText, askText } from "@/lib/sessions/bus"
import { insertMessage, inSession, messageWithSession } from "@/lib/sessions/messages"
import { loadSessionForUser } from "@/lib/sessions/access"
import { parseSealedValue } from "@exp/domain-contract/sealed-content"
import { isSealedCard, validSealedCard } from "@/lib/sessions/sealed"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1399: ESCALATE a waiting question / plan card into the team agent's
// conversation that manages its thread. The escalated card is an `ask`
// mirror ({sessionId, messageId}, like the bus's), so every client draws
// THE origin card there (session-thread.json): one row, answered through
// `sessions.answer(origin)` with the origin's own auth, first answer wins,
// and `closeAskMirrors` closes the mirror with it. A root's own cards reach
// the conversation that started it already (it watches the root); this op
// lifts a card from anywhere in that tree. EXP-1458: a SEALED origin card
// is never read: the mirror's body is the neutral `askHiddenText` (the
// clear `kind` and the session id only), like a hidden listener's; the
// device may pass the card sealed to the TARGET tree (`sealedCard`, after
// `escalateTarget`), which rides on the mirror as `card.sealedCard`.

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

/** The agent conversation managing `origin`'s tree (`root`): a live
 *  agent conversation watching the root, the starter's own first, else
 *  the root's starting agent's live conversation with the root's starter.
 *  #EXP-1399 review: never one whose readers may not see `origin` (its
 *  account must see it; a shared conversation only for a shared origin). */
export async function managingConversation(
  tx: Pick<Tx, `select`>,
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

/** The card `messageId` names (its origin when it is an `ask` mirror), its
 *  session (the user must see it) and that session's root. */
async function loadEscalation(userId: string, messageId: string) {
  let [message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, messageId)).limit(1)
  // An `ask` mirror (the bus's, in a parent) escalates its origin card.
  const mirrored = message?.card as { kind?: unknown; messageId?: unknown } | null | undefined
  if (message && mirrored?.kind === `ask` && typeof mirrored.messageId === `string`) {
    ;[message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, mirrored.messageId)).limit(1)
  }
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const origin = await loadSessionForUser(userId, message.sessionId)
  const [root] = await db.select().from(sessions).where(eq(sessions.id, origin.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  return { message, origin, root }
}

/** The checks every escalation makes on the (locked) card row; returns the
 *  card and the managing conversation it lands in. */
async function escalationTarget(
  database: Pick<Tx, `select`>,
  row: { card: unknown; answeredAt: Date | null } | undefined,
  root: Session,
  origin: Session
): Promise<{ card: Extract<MessageCard, { kind: `question` | `plan` }>; target: Session }> {
  const card = row?.card as MessageCard | null | undefined
  if (card?.kind !== `question` && card?.kind !== `plan`) {
    throw bad(`messageId must name a question or plan card (or its ask mirror)`)
  }
  if (row!.answeredAt) throw bad(`That card is already answered`)
  const target = await managingConversation(database, root, origin)
  if (!target) {
    throw bad(`No team agent manages this thread: escalate works in a thread an agent's conversation started or watches`)
  }
  if (target.id === root.id) throw bad(`That card is already in the agent's conversation`)
  return { card, target }
}

/** EXP-1458 (addendum 3): the ROOT of the tree an escalation of
 *  `messageId` would land in (the managing conversation), so the device
 *  seals the card's copy to that tree's key (`sessionKeys.publicKeys`)
 *  before it escalates with `sealedCard`. The same checks as escalating,
 *  minus the caller session's (a person's device asks). */
export async function escalateTarget(userId: string, messageId: string): Promise<{ rootId: string }> {
  const { message, origin, root } = await loadEscalation(userId, messageId)
  const [row] = await db
    .select({ card: messages.card, answeredAt: messages.answeredAt })
    .from(messages)
    .where(eq(messages.id, message.id))
    .limit(1)
  const { target } = await escalationTarget(db, row, root, origin)
  return { rootId: target.rootId }
}

/** EXP-1458: the device's copy of a SEALED origin card for the target tree:
 *  a valid envelope of the origin's kind, at an epoch the target root has. */
function assertSealedCopy(
  sealedCard: Record<string, unknown>,
  card: MessageCard,
  target: Pick<Session, `sealEpoch`>
): void {
  if (!isSealedCard(card)) throw bad(`This card is not sealed: escalate it without sealedCard`)
  if (!validSealedCard(sealedCard)) {
    throw bad(`sealedCard must be {kind, sealed: "v1:<epoch>:<b64url>"} plus clear fields only`)
  }
  if (sealedCard.kind !== card.kind) throw bad(`sealedCard is a ${sealedCard.kind} card; this one is a ${card.kind}`)
  const value = parseSealedValue(sealedCard.sealed as string)
  if (value.kind !== `sealed`) return
  if (target.sealEpoch == null) throw bad(`The agent's conversation has no key for sealed cards yet`)
  if (value.epoch > target.sealEpoch) throw bad(`The agent's conversation's key is at epoch ${target.sealEpoch}`)
}

export async function escalateCard(input: {
  userId: string
  /** The calling session (in the card's tree, or the managing conversation). */
  caller: Pick<Session, `id` | `rootId`>
  messageId: string
  /** EXP-1458 (addendum 3): a SEALED origin card's copy, sealed by the
   *  device to the TARGET tree (`escalateTarget`): `{...clear, sealed}` of
   *  the origin's kind. It rides on the mirror as `card.sealedCard` (the
   *  mirror stays the clear `ask` routing row); absent = the neutral line
   *  only. Refused for a plaintext card. */
  sealedCard?: Record<string, unknown> | null
}): Promise<{ messageId: string; sessionId: string; escalated: boolean }> {
  const { message, origin, root } = await loadEscalation(input.userId, input.messageId)

  return db.transaction(async (tx) => {
    // The card row is THE lock: an answer racing this either lands first
    // (refused below) or waits and then closes the mirror inserted here;
    // a second escalate waits and finds the first one's mirror.
    const [locked] = await tx.select().from(messages).where(eq(messages.id, message.id)).limit(1).for(`update`)
    const { card, target } = await escalationTarget(tx, locked, root, origin)
    if (input.caller.rootId !== root.id && input.caller.rootId !== target.id) {
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `Escalate a card of your own thread, or one of a thread your conversation manages`,
      })
    }
    if (input.sealedCard) assertSealedCopy(input.sealedCard, card, target)
    const [existing] = await tx
      .select({ id: messages.id })
      .from(messages)
      .where(
        and(
          inSession(target.id),
          sql`${messages.card}->>'kind' = 'ask'`,
          sql`${messages.card}->>'messageId' = ${message.id}`
        )
      )
      .limit(1)
    if (existing) return { messageId: existing.id, sessionId: target.id, escalated: false }
    const hidden = askHidden(target, origin)
    const sealed = isSealedCard(card)
    // A hidden target's readers never get the copy (they may not see it).
    const copy = sealed && !hidden && input.sealedCard ? input.sealedCard : null
    const id = await insertMessage(tx, target, {
      authorKind: `agent`,
      authorSessionId: origin.id,
      body: sealed || hidden ? askHiddenText(origin, card) : askText(origin, card),
      card: {
        kind: `ask`,
        sessionId: origin.id,
        messageId: message.id,
        ...(copy ? { sealedCard: copy } : {}),
      } as MessageCard,
      // It wakes the agent, unless the agent escalated it itself.
      consumedAt: input.caller.id === target.id || target.status === `ended` ? new Date() : null,
    })
    return { messageId: id, sessionId: target.id, escalated: true }
  })
}
