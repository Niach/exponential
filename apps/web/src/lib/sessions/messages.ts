import { and, desc, eq, getTableColumns, gt, inArray, isNotNull, isNull, ne, or, sql, type AnyColumn, type SQL } from "drizzle-orm"
import type { MessageAuthorKind, MessageCard } from "@exp/db-schema/domain"
import { conversations, messages, sessions, type Session } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// EXP-1291: every Conversation row goes through `insertMessage`. The
// visibility mirrors (`root_id`, `team_id`, `shared`, `owner_id` = the
// session's starter) are written here AND re-derived by the
// `populate_message_session_mirrors` trigger (the trigger is the truth; the
// app values only satisfy the NOT NULL columns).

type SessionMirror = Pick<Session, `id` | `rootId` | `teamId` | `shared` | `userId`>

async function sessionMirror(tx: Tx, session: string | SessionMirror): Promise<SessionMirror> {
  if (typeof session !== `string`) return session
  const [row] = await tx
    .select({
      id: sessions.id,
      rootId: sessions.rootId,
      teamId: sessions.teamId,
      shared: sessions.shared,
      userId: sessions.userId,
    })
    .from(sessions)
    .where(eq(sessions.id, session))
    .limit(1)
  if (!row) throw new Error(`Session ${session} not found`)
  return row
}

/** NEW-3: the session's own conversation (kind `session`, created with the
 *  session by the `create_session_conversation` trigger). */
export async function sessionConversationId(tx: Tx, sessionId: string): Promise<string> {
  const [row] = await tx
    .select({ id: conversations.id })
    .from(conversations)
    .where(eq(conversations.sessionId, sessionId))
    .limit(1)
  if (!row) throw new Error(`Session ${sessionId} has no conversation`)
  return row.id
}

// NEW-8: a message names only its conversation; its session is the
// conversation's (`conversations.session_id`, kind `session`, one each; a
// dm/channel conversation has none). Both directions are scalar subqueries
// on a unique key, so a session filter still walks
// `idx_messages_conversation_created`.

/** The session's conversation id (a scalar subquery). */
export function sessionConversation(sessionId: string | AnyColumn | SQL): SQL<string> {
  return sql<string>`(SELECT ${conversations.id} FROM ${conversations} WHERE ${conversations.sessionId} = ${sessionId})`
}

/** Messages of one session. */
export function inSession(sessionId: string | AnyColumn | SQL): SQL {
  return eq(messages.conversationId, sessionConversation(sessionId))
}

/** Messages of any of these sessions (none = matches nothing). */
export function inSessions(sessionIds: readonly string[]): SQL {
  return inArray(
    messages.conversationId,
    sql`(SELECT ${conversations.id} FROM ${conversations} WHERE ${inArray(conversations.sessionId, [...sessionIds])})`
  )
}

/** A message's session (its conversation's; NULL only in a sessionless
 *  dm/channel conversation, which never carries session cards). */
export function messageSessionId(): SQL<string> {
  return sql<string>`(SELECT ${conversations.sessionId} FROM ${conversations} WHERE ${conversations.id} = ${messages.conversationId})`
}

/** A full message row plus its session (`messageSessionId`). */
export function messageWithSession() {
  return { ...getTableColumns(messages), sessionId: messageSessionId() }
}

export async function insertMessage(
  tx: Tx,
  session: string | SessionMirror,
  message: {
    authorKind: MessageAuthorKind
    authorId?: string | null
    /** EXP-1323: the session that posted it (a child's report/question). */
    authorSessionId?: string | null
    body?: string
    card?: MessageCard | null
    /** Pre-consumed: an agent's row that must never wake anything. */
    consumedAt?: Date | null
    /** S-3: the row this one is a bus copy of (patched on its edit). */
    originMessageId?: string | null
  }
): Promise<string> {
  const mirror = await sessionMirror(tx, session)
  const [row] = await tx
    .insert(messages)
    .values({
      conversationId: await sessionConversationId(tx, mirror.id),
      rootId: mirror.rootId,
      teamId: mirror.teamId,
      shared: mirror.shared,
      ownerId: mirror.userId,
      authorKind: message.authorKind,
      authorId: message.authorId ?? null,
      authorSessionId: message.authorSessionId ?? null,
      body: message.body ?? ``,
      card: message.card ?? null,
      consumedAt: message.consumedAt ?? null,
      originMessageId: message.originMessageId ?? null,
    })
    .returning({ id: messages.id })
  return row!.id
}

/** A server event card (issue linked, PR opened, a child session …). */
export function postSystemCard(
  tx: Tx,
  sessionId: string | SessionMirror,
  card: MessageCard
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `system`, card })
}

/** EXP-1252: a server line in the Conversation (a session moved devices, a
 *  move failed). Pre-consumed: it never wakes or feeds the agent. */
export function postMovedMessage(
  tx: Tx,
  sessionId: string | SessionMirror,
  body: string
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `system`, body, consumedAt: new Date() })
}

/** The agent's side: a turn's final reply text and/or a card. */
export function postAgentMessage(
  tx: Tx,
  sessionId: string | SessionMirror,
  body: string,
  card?: MessageCard | null
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `agent`, body, card: card ?? null })
}

/**
 * S-3 "one text + one card between two person messages": in an AGENT
 * conversation a turn's reply text EDITS the agent's text already posted
 * since the last DELIVERED input (the trigger tick and a person's line are
 * both person-authored; a line still queued belongs to the next turn) instead of landing as another bubble. → the edited
 * row and its previous body, or null (nothing to edit: insert). Its bus
 * copies follow (`patchBusCopies`).
 */
export async function editAgentTurnText(
  tx: Tx,
  sessionId: string,
  body: string
): Promise<{ id: string; before: string } | null> {
  // The turn starts at the newest input the device DELIVERED (its
  // `consumed_at`): a message still queued mid-turn is the NEXT turn's, so
  // it must not cut this turn's answer off (F29).
  const [delivered] = await tx
    .select({ at: messages.consumedAt })
    .from(messages)
    .where(and(inSession(sessionId), pendingInput(), isNotNull(messages.consumedAt)))
    .orderBy(desc(messages.consumedAt))
    .limit(1)
  const [text] = await tx
    .select({ id: messages.id, body: messages.body })
    .from(messages)
    .where(
      and(
        inSession(sessionId),
        eq(messages.authorKind, `agent`),
        isNull(messages.card),
        isNull(messages.authorSessionId),
        ...(delivered?.at ? [gt(messages.createdAt, delivered.at)] : [])
      )
    )
    .orderBy(desc(messages.createdAt))
    .limit(1)
    .for(`update`)
  if (!text) return null
  await tx.update(messages).set({ body, updatedAt: new Date() }).where(eq(messages.id, text.id))
  return { id: text.id, before: text.body }
}

/** S-3: an edited row's bus copies take its new text. Only a copy still
 *  carrying the old text: a neutral private line (#EXP-1408, a reader who
 *  may not see the origin) never gains the text. */
export async function patchBusCopies(tx: Tx, originId: string, before: string, after: string): Promise<void> {
  await tx
    .update(messages)
    .set({ body: after, updatedAt: new Date() })
    .where(and(eq(messages.originMessageId, originId), eq(messages.body, before.trim())))
}

/** A person's message: the bus (consumed_at NULL until the device delivers it).
 *  `authorSessionId` = the session whose MCP key sent it on the person's behalf. */
export function postPersonMessage(
  tx: Tx,
  sessionId: string | SessionMirror,
  authorId: string,
  body: string,
  authorSessionId?: string | null
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `user`, authorId, body, authorSessionId })
}

/** EXP-1323: another SESSION's input into this one (a child asking its
 *  parent, a child's report): agent-authored, yet pending like a person's
 *  message (`consumed_at` NULL) so an idle session resumes and reads it. */
export function postSessionInput(
  tx: Tx,
  sessionId: string | SessionMirror,
  fromSessionId: string,
  body: string,
  card?: MessageCard | null
): Promise<string> {
  return insertMessage(tx, sessionId, {
    authorKind: `agent`,
    authorSessionId: fromSessionId,
    body,
    card: card ?? null,
  })
}

/** EXP-1323: THE pending-input predicate (the device feeds these, `consume`
 *  stamps them): a person's message, or another session's input. An
 *  agent's own turn reply carries no `author_session_id` and never waits;
 *  EXP-1280: neither does a card the session posted on ITSELF (its
 *  `author_session_id` = its own id; `answeredOwnCard` once answered). */
export function pendingInput(): SQL {
  return or(
    eq(messages.authorKind, `user`),
    and(isNotNull(messages.authorSessionId), ne(messages.authorSessionId, messageSessionId()))
  )!
}

/** EXP-1280: the session's own plan/question card (`postOwnAskCard`) once a
 *  person answered it: the device delivers the answer text, then consumes it. */
export function answeredOwnCard(): SQL {
  return and(
    eq(messages.authorSessionId, messageSessionId()),
    isNotNull(messages.answeredAt),
    sql`${messages.card}->>'kind' IN ('question', 'plan')`
  )!
}

/** EXP-1280: a card the session posts on ITSELF over MCP (`plan_propose`,
 *  `sessions_ask`): agent-authored, `author_session_id` = the session,
 *  `consumed_at` NULL until the device delivers the person's answer, and the
 *  session waits on a person (`needs_input`) in the same transaction. */
export async function postOwnAskCard(
  tx: Tx,
  session: SessionMirror,
  card: MessageCard
): Promise<string> {
  const messageId = await insertMessage(tx, session, {
    authorKind: `agent`,
    authorSessionId: session.id,
    card,
  })
  await tx
    .update(sessions)
    .set({ needsInput: true, updatedAt: new Date() })
    .where(eq(sessions.id, session.id))
  return messageId
}
