import { TRPCError } from "@trpc/server"
import { and, desc, eq, isNull, sql } from "drizzle-orm"
import { z } from "zod"
import {
  MESSAGE_BODY_MAX,
  PLAN_CARD_LABEL_MAX,
  planCardAnswersSchema,
  QUESTION_CARD_OPTIONS_MAX,
  type MessageCard,
  type SessionAnswerInput,
} from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { messages, sessions, type Session } from "@/db/schema"
import { clientAtOrBelow } from "@/lib/client-version"
import { generateTxId } from "@/lib/trpc"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { answeredCard } from "@/lib/sessions/card-answer"
import type { Db, Tx } from "@/lib/sessions/tx"
import { closeAskMirrors } from "@/lib/sessions/bus"
import { isVoiceConversation } from "@/lib/sessions/brief"

// EXP-1280/NEW-1: answering a question/plan card. ONE write path for the
// thread card (`sessions.answer`, a person), the device's write-back
// (`sessions.resolveCard`) and a parent's or watcher's answer over the bus
// (`sessions.send({sessionId, answer})`): the answer is stamped IN the card
// with `answered_at`, under an "unanswered" guard, so the FIRST answer wins
// and every later one is CONFLICT. The device delivers the answer text to
// the agent off the messages shape; no person message is posted.

const uuid = z.string().uuid()

/** #NEW-79: the refusal when anyone but its member answers a voice card. */
export const VOICE_CARD_OWNER_ONLY = `Only the member whose voice conversation this is can answer its cards`

export const cardAnswerInput = z.object({
  messageId: uuid,
  answer: z.string().max(MESSAGE_BODY_MAX).optional(),
  decision: z.enum([`approved`, `rejected`]).optional(),
  fresh: z.boolean().optional(),
  feedback: z.string().max(MESSAGE_BODY_MAX).optional(),
  // EXP-1280: a plan's answers by question id, a question's picked labels;
  // the inline surface's data model at answer time.
  answers: z
    .union([
      planCardAnswersSchema,
      z.array(z.string().max(PLAN_CARD_LABEL_MAX)).max(QUESTION_CARD_OPTIONS_MAX),
    ])
    .optional(),
  surfaceData: z.record(z.string(), z.unknown()).optional(),
})
export type CardAnswerInput = z.infer<typeof cardAnswerInput>

// COMPAT (EXP-1280): delete once CLIENT_MIN_VERSION_IOS > 0.14.62 AND
// CLIENT_MIN_VERSION_ANDROID > 0.14.64 AND CLIENT_MIN_VERSION_DESKTOP/_CLI >
// 0.14.72 (the last clients shipped before the rich plan card's questions).
// Their Approve sends a decision and no `answers`: on a plan with questions
// that is a plain approval (the agent reads the plan as approved, the
// questions unanswered), never the `missing` refusal a current client's
// form would get for leaving one blank.
export const PLAN_ANSWERS_LEGACY_CLIENTS = {
  ios: `0.14.62`,
  android: `0.14.64`,
  desktop: `0.14.72`,
  cli: `0.14.72`,
} as const

/** The card message + its session (steer-gated) + the answered card. */
export async function loadCardForAnswer(
  database: Db,
  userId: string,
  input: CardAnswerInput,
  request?: Pick<Request, `headers`> | null
) {
  const [message] = await database
    .select()
    .from(messages)
    .where(eq(messages.id, input.messageId))
    .limit(1)
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const session = await loadSessionForUser(userId, message.sessionId)
  await assertCanSteer(userId, session)
  // #NEW-79: a voice conversation's proposal cards approve calls made in
  // its member's name: only that member answers them.
  if (session.userId !== userId && (await isVoiceConversation(session, database))) {
    throw new TRPCError({ code: `FORBIDDEN`, message: VOICE_CARD_OWNER_ONLY })
  }
  const legacyPlanApproval = !!request && clientAtOrBelow(request, PLAN_ANSWERS_LEGACY_CLIENTS)
  const answered = answeredCard(message.card as MessageCard | null, {
    ...input,
    legacyPlanApproval,
  })
  if (!answered.ok) throw new TRPCError({ code: answered.code, message: answered.message })
  return { message, session, card: answered.card }
}

/** Stamp `card` + `answered_at` while still unanswered; needs-you clears
 *  once nothing else waits on a person. `claimed` = this call stamped it.
 *  EXP-1280: an approved plan takes the session out of plan mode (the
 *  `pr_open` gate opens) in the same transaction. */
export async function stampAnsweredCardInTx(
  tx: Tx,
  messageId: string,
  sessionId: string,
  card: MessageCard
): Promise<boolean> {
  const now = new Date()
  const claimed = await tx
    .update(messages)
    .set({ card, answeredAt: now, updatedAt: now })
    .where(and(eq(messages.id, messageId), isNull(messages.answeredAt)))
    .returning({ id: messages.id })
  if (claimed.length === 0) return false
  await closeAskMirrors(tx, sessionId, [messageId], now)
  if (card.kind === `plan` && card.decision === `approved`) {
    await tx
      .update(sessions)
      .set({ planMode: false, updatedAt: now })
      .where(and(eq(sessions.id, sessionId), eq(sessions.planMode, true)))
  }
  const [{ pending } = { pending: 0 }] = await tx
    .select({ pending: sql<number>`count(*)::int` })
    .from(messages)
    .where(
      and(
        eq(messages.sessionId, sessionId),
        isNull(messages.answeredAt),
        sql`${messages.card}->>'kind' IN ('question', 'plan')`
      )
    )
  if (Number(pending) === 0) {
    // NEW-1: a blocked session stays under Needs you whatever waits.
    await tx
      .update(sessions)
      .set({ needsInput: false, updatedAt: now })
      .where(and(eq(sessions.id, sessionId), isNull(sessions.blocked)))
  }
  return true
}

export async function stampAnsweredCard(
  database: typeof db,
  messageId: string,
  sessionId: string,
  card: MessageCard
): Promise<{ claimed: boolean; txid: number }> {
  return database.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const claimed = await stampAnsweredCardInTx(tx, messageId, sessionId, card)
    return { claimed, txid }
  })
}

/**
 * NEW-1: `sessions.send({sessionId, answer})`: answer the target's open
 * card (`messageId`, else its newest unanswered question/plan) inside the
 * send's transaction. CONFLICT once answered (the first answer wins), NOT
 * FOUND with nothing open.
 */
export async function answerSessionCardInTx(
  tx: Tx,
  target: Pick<Session, `id` | `rootId` | `agentId` | `userId`>,
  input: SessionAnswerInput
): Promise<{ messageId: string }> {
  // #NEW-79: a session never answers a voice conversation's proposal card
  // (an agent would approve its own calls); its member does, in person.
  if (await isVoiceConversation(target, tx)) {
    throw new TRPCError({ code: `FORBIDDEN`, message: VOICE_CARD_OWNER_ONLY })
  }
  const [message] = await tx
    .select({ id: messages.id, card: messages.card, answeredAt: messages.answeredAt })
    .from(messages)
    .where(
      and(
        eq(messages.sessionId, target.id),
        sql`${messages.card}->>'kind' IN ('question', 'plan')`,
        ...(input.messageId ? [eq(messages.id, input.messageId)] : [isNull(messages.answeredAt)])
      )
    )
    .orderBy(desc(messages.createdAt), desc(messages.id))
    .limit(1)
    .for(`update`)
  if (!message) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `This session has no open question or plan` })
  }
  if (message.answeredAt) throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
  const answered = answeredCard(message.card as MessageCard | null, input)
  if (!answered.ok) throw new TRPCError({ code: answered.code, message: answered.message })
  const claimed = await stampAnsweredCardInTx(tx, message.id, target.id, answered.card)
  if (!claimed) throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
  return { messageId: message.id }
}
