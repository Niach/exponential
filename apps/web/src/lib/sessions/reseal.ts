// #EDE-97 (fixtures/sealed-content.json `clearTree`, RESEAL): a session's
// first message the server wrote before its tree had a key (a trigger or
// schedule firing, an app hand-off, a suggestion started, a raw start) is
// re-sealed ONCE by the device executing it: `resealTarget` names it,
// `reseal` replaces its stored body with the envelope. Its routing (mentions,
// attachment and ref ids, `issue_brief`, the card) stays as it is.

import { parseBodyEnvelope } from "@exp/domain-contract/sealed-content"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, isNull, ne, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { messages, sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { isSealedBody, validSealedBody } from "@/lib/sessions/sealed"
import type { Tx } from "@/lib/sessions/tx"

type Reader = Pick<Tx, `select`>

export const RESEAL_REFUSED_MESSAGE = `This message cannot be re-sealed`

const refused = () => new TRPCError({ code: `BAD_REQUEST`, message: RESEAL_REFUSED_MESSAGE })

/** The executing device only (the claim's rule). */
function assertExecutingDevice(session: Pick<Session, `deviceId`>, deviceId: string) {
  if (session.deviceId !== deviceId) {
    throw new TRPCError({ code: `CONFLICT`, message: `This session runs on another device` })
  }
}

/** The tree's current epoch when it is keyed and not clear, else null. */
async function sealingEpoch(database: Reader, rootId: string): Promise<number | null> {
  const [root] = await database
    .select({ sealEpoch: sessions.sealEpoch, clearTree: sessions.clearTree })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  if (!root || root.clearTree || root.sealEpoch == null) return null
  return root.sealEpoch
}

/** The session's first message while it is still re-sealable: author a
 *  person or an agent, its body plaintext and not blank, and no reply of
 *  this session's agent after it. */
async function firstPlainMessage(database: Reader, sessionId: string): Promise<{ id: string; body: string } | null> {
  const [first] = await database
    .select({ id: messages.id, body: messages.body, authorKind: messages.authorKind })
    .from(messages)
    .where(eq(messages.sessionId, sessionId))
    .orderBy(asc(messages.createdAt), asc(messages.id))
    .limit(1)
  if (!first || first.authorKind === `system` || !first.body.trim() || isSealedBody(first.body)) return null
  const [replied] = await database
    .select({ id: messages.id })
    .from(messages)
    .where(
      and(
        eq(messages.sessionId, sessionId),
        eq(messages.authorKind, `agent`),
        // Its agent's replies carry no author session (or its own).
        or(isNull(messages.authorSessionId), eq(messages.authorSessionId, sessionId)),
        ne(messages.id, first.id)
      )
    )
    .limit(1)
  return replied ? null : { id: first.id, body: first.body }
}

/** `sessions.resealTarget`: what the executing device re-seals, or null. */
export async function resealTarget(
  session: Pick<Session, `id` | `rootId` | `deviceId`>,
  deviceId: string
): Promise<{ messageId: string; body: string } | null> {
  assertExecutingDevice(session, deviceId)
  if ((await sealingEpoch(db, session.rootId)) == null) return null
  const first = await firstPlainMessage(db, session.id)
  return first ? { messageId: first.id, body: first.body } : null
}

/** `sessions.reseal`: replaces the first message's plaintext with `body`, an
 *  envelope at the tree's current epoch. Once: a sealed body is refused. */
export async function reseal(
  session: Pick<Session, `id` | `rootId` | `deviceId`>,
  input: { deviceId: string; messageId: string; body: string }
): Promise<{ txid: number }> {
  assertExecutingDevice(session, input.deviceId)
  if (!isSealedBody(input.body) || !validSealedBody(input.body)) throw refused()
  const envelope = parseBodyEnvelope(input.body)
  return db.transaction(async (tx) => {
    // The message row first: a concurrent re-seal waits and then sees it sealed.
    await tx.execute(sql`SELECT id FROM messages WHERE id = ${input.messageId} FOR UPDATE`)
    const epoch = await sealingEpoch(tx, session.rootId)
    if (epoch == null || envelope.kind !== `sealed` || envelope.epoch !== epoch) throw refused()
    const first = await firstPlainMessage(tx, session.id)
    if (!first || first.id !== input.messageId) throw refused()
    const txid = await generateTxId(tx)
    await tx.update(messages).set({ body: input.body, updatedAt: new Date() }).where(eq(messages.id, input.messageId))
    return { txid }
  })
}
