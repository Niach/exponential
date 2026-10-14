import { eq } from "drizzle-orm"
import type { MessageAuthorKind, MessageCard } from "@exp/db-schema/domain"
import { messages, sessions, type Session } from "@/db/schema"
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

export async function insertMessage(
  tx: Tx,
  session: string | SessionMirror,
  message: {
    authorKind: MessageAuthorKind
    authorId?: string | null
    body?: string
    card?: MessageCard | null
  }
): Promise<string> {
  const mirror = await sessionMirror(tx, session)
  const [row] = await tx
    .insert(messages)
    .values({
      sessionId: mirror.id,
      rootId: mirror.rootId,
      teamId: mirror.teamId,
      shared: mirror.shared,
      ownerId: mirror.userId,
      authorKind: message.authorKind,
      authorId: message.authorId ?? null,
      body: message.body ?? ``,
      card: message.card ?? null,
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

/** The agent's side: a turn's final reply text and/or a card. */
export function postAgentMessage(
  tx: Tx,
  sessionId: string | SessionMirror,
  body: string,
  card?: MessageCard | null
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `agent`, body, card: card ?? null })
}

/** A person's message: the bus (consumed_at NULL until the device delivers it). */
export function postPersonMessage(
  tx: Tx,
  sessionId: string | SessionMirror,
  authorId: string,
  body: string
): Promise<string> {
  return insertMessage(tx, sessionId, { authorKind: `user`, authorId, body })
}
