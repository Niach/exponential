import { and, eq, inArray, ne, or, sql } from "drizzle-orm"
import { messages, sessionReads, sessions } from "@/db/schema"
import type { Db } from "@/lib/sessions/tx"
import { inSession } from "@/lib/sessions/messages"

// #EXP-1452: an agent's read is its member's read (chats.json READS). The
// member whose agent read a chat (a bus line its session consumed, a
// `sessions get` of another tree, a message its session sent into one) has
// read that chat up to then: the ROOT's `session_reads.read_at` moves
// forward to `at`, never back, and only on a chat that member may see.
// Never the agent's OWN tree: a child's line in it is still news.

/** `userId` read the trees of `rootIds` up to `at` (a root they may not
 *  see, or of another team than `teamId`, is skipped). */
export async function markTreesReadBy(
  tx: Db,
  userId: string,
  teamId: string,
  rootIds: readonly string[],
  at: Date
): Promise<void> {
  const wanted = [...new Set(rootIds)]
  if (wanted.length === 0) return
  const visible = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        inArray(sessions.id, wanted),
        eq(sessions.teamId, teamId),
        or(eq(sessions.shared, true), eq(sessions.userId, userId))
      )
    )
  const now = new Date()
  for (const { id } of visible) {
    await tx
      .insert(sessionReads)
      .values({ userId, sessionId: id, readAt: at })
      .onConflictDoUpdate({
        target: [sessionReads.userId, sessionReads.sessionId],
        set: {
          readAt: sql`greatest(${sessionReads.readAt}, ${at.toISOString()}::timestamptz)`,
          updatedAt: now,
        },
      })
  }
}

/** The device delivered `messageIds` into `reader`: every one a session
 *  of another tree authored marks that chat read for `reader`'s member, up
 *  to the line. */
export async function markConsumedLinesRead(
  tx: Db,
  reader: { id: string; rootId: string; teamId: string; userId: string },
  messageIds: readonly string[]
): Promise<void> {
  if (messageIds.length === 0) return
  const lines = await tx
    .select({ rootId: sessions.rootId, createdAt: messages.createdAt })
    .from(messages)
    .innerJoin(sessions, eq(sessions.id, messages.authorSessionId))
    .where(
      and(
        inSession(reader.id),
        inArray(messages.id, [...messageIds]),
        ne(sessions.rootId, reader.rootId)
      )
    )
  const newest = new Map<string, Date>()
  for (const line of lines) {
    const seen = newest.get(line.rootId)
    if (!seen || line.createdAt > seen) newest.set(line.rootId, line.createdAt)
  }
  for (const [rootId, at] of newest) {
    await markTreesReadBy(tx, reader.userId, reader.teamId, [rootId], at)
  }
}
