import { and, asc, desc, eq, gt, inArray, isNull, ne, or } from "drizzle-orm"
import { BRIEF_RECENT_MAX, type BriefCall } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { issues, messages, sessionIssues, sessions, users, type Session } from "@/db/schema"
import { insertMessage, inSession } from "@/lib/sessions/messages"
import { isSealedBody } from "@/lib/sessions/sealed"
import { deriveSessionTitle } from "@/lib/sessions/title"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1961 (etalk voice-call.json `callSession`): DURING THE CALL.

/** Before a person's message lands in a call session: the chat's messages
 *  newer than `readUpTo` (a person's, the chat agent's own plain replies;
 *  no cards, nothing the call wrote) become its pending input, authored by
 *  the chat, so the call reads them before its next answer. A sealed body
 *  is a byte copy (same tree, same key). */
export async function copyChatIntoCall(
  tx: Tx,
  call: Pick<Session, `id` | `parentId` | `voiceCall`>
): Promise<void> {
  if (!call.voiceCall || !call.parentId) return
  const since = new Date(call.voiceCall.readUpTo)
  const rows = await tx
    .select({ id: messages.id, body: messages.body, createdAt: messages.createdAt })
    .from(messages)
    .where(
      and(
        inSession(call.parentId),
        gt(messages.createdAt, since),
        isNull(messages.card),
        or(
          eq(messages.authorKind, `user`),
          and(eq(messages.authorKind, `agent`), isNull(messages.authorSessionId))
        ),
        or(isNull(messages.authorSessionId), ne(messages.authorSessionId, call.id))
      )
    )
    .orderBy(asc(messages.createdAt))
  const now = new Date()
  // `readUpTo` is in ms, Postgres in µs: the last copied row can come back.
  const copied = new Set(
    rows.length === 0
      ? []
      : (
          await tx
            .select({ originMessageId: messages.originMessageId })
            .from(messages)
            .where(and(inSession(call.id), inArray(messages.originMessageId, rows.map((row) => row.id))))
        ).map((row) => row.originMessageId)
  )
  for (const row of rows) {
    if (!row.body.trim() || copied.has(row.id)) continue
    await insertMessage(tx, call.id, {
      authorKind: `agent`,
      authorSessionId: call.parentId,
      body: row.body,
      consumedAt: null,
      originMessageId: row.id,
    })
  }
  const readUpTo = rows.length > 0 ? rows.at(-1)!.createdAt : now
  await tx
    .update(sessions)
    .set({ voiceCall: { ...call.voiceCall, readUpTo: new Date(Math.max(readUpTo.getTime(), since.getTime())).toISOString() } })
    .where(eq(sessions.id, call.id))
}

/** The Call section of a call session's brief (session-brief.json
 *  `callCases`): the chat, its recent plaintext messages, its open work. */
export async function callBrief(session: Pick<Session, `parentId` | `userId`>): Promise<BriefCall | null> {
  if (!session.parentId) return null
  const [chat] = await db.select().from(sessions).where(eq(sessions.id, session.parentId)).limit(1)
  if (!chat) return null
  const rows = await db
    .select({ body: messages.body, authorKind: messages.authorKind, authorId: messages.authorId })
    .from(messages)
    .where(and(inSession(chat.id), isNull(messages.card), isNull(messages.authorSessionId), inArray(messages.authorKind, [`user`, `agent`])))
    .orderBy(desc(messages.createdAt))
    .limit(BRIEF_RECENT_MAX * 2)
  const authorIds = [...new Set(rows.flatMap((row) => (row.authorId ? [row.authorId] : [])))]
  const names = new Map(
    authorIds.length > 0
      ? (await db.select({ id: users.id, name: users.name }).from(users).where(inArray(users.id, authorIds))).map(
          (row) => [row.id, row.name] as const
        )
      : []
  )
  const recent = rows
    .filter((row) => row.body.trim() && !isSealedBody(row.body))
    .slice(0, BRIEF_RECENT_MAX)
    .reverse()
    .map((row) => ({
      who: row.authorKind === `agent` ? `You` : ((row.authorId && names.get(row.authorId)) || `Person`),
      text: row.body,
    }))
  const issueRows = await db
    .select({ identifier: issues.identifier, title: issues.title })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(and(eq(sessionIssues.sessionId, chat.id), isNull(issues.deletedAt), isNull(issues.completedAt)))
    .orderBy(asc(sessionIssues.sortOrder))
  const children = await db
    .select()
    .from(sessions)
    .where(and(eq(sessions.parentId, chat.id), ne(sessions.status, `ended`), isNull(sessions.voiceCall), isNull(sessions.subagentKey)))
    .orderBy(asc(sessions.createdAt))
  const work = [
    ...issueRows.map((row) => `${row.identifier} ${row.title}`),
    ...(await Promise.all(children.map(async (child) => `session "${await deriveSessionTitle(child)}" (${child.status})`))),
  ]
  return { title: await deriveSessionTitle(chat), chatId: chat.id, recent, work }
}
