// #EXP-2028 (access.json `actingSession`): the launcher's key of a plain
// chat's session is BOUND to that session (`users.mintPersonalApiKey`
// `sessionId`). A session holds at most one live bound key: minting again
// for it replaces the earlier one, and every mint drops the member's keys
// bound to ended or vanished sessions (a run's key dies with its session;
// a resumed session's launch mints a fresh one).
import { and, eq, inArray, like, ne } from "drizzle-orm"
import { apikeys } from "@/db/auth-schema"
import { sessions } from "@/db/schema"
import { parseApiKeyMetadata } from "@/lib/auth/api-key-kind"
import type { Context } from "@/lib/trpc"

/** True when a stored key row is bound to a session (never listed). */
export function isSessionBoundKey(metadata: string | null | undefined): boolean {
  return Boolean(parseApiKeyMetadata(metadata).sessionId)
}

/** Delete `userId`'s session-bound keys other than `keepKeyId` that are
 *  bound to `replacedSessionId`, or to a session that ended or no longer
 *  exists. Returns how many went. */
export async function sweepSessionBoundKeys(
  db: Context[`db`],
  userId: string,
  replacedSessionId: string,
  keepKeyId: string
): Promise<number> {
  const rows = await db
    .select({ id: apikeys.id, metadata: apikeys.metadata })
    .from(apikeys)
    .where(and(eq(apikeys.referenceId, userId), ne(apikeys.id, keepKeyId), like(apikeys.metadata, `%"sessionId"%`)))
  const bound = rows.flatMap((row) => {
    const sessionId = parseApiKeyMetadata(row.metadata).sessionId
    return sessionId ? [{ id: row.id, sessionId }] : []
  })
  if (bound.length === 0) return 0
  const live = await db
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(inArray(sessions.id, [...new Set(bound.map((row) => row.sessionId))]), ne(sessions.status, `ended`)))
  const liveIds = new Set(live.map((row) => row.id))
  const doomed = bound.filter((row) => row.sessionId === replacedSessionId || !liveIds.has(row.sessionId)).map((row) => row.id)
  if (doomed.length === 0) return 0
  await db.delete(apikeys).where(and(eq(apikeys.referenceId, userId), inArray(apikeys.id, doomed)))
  return doomed.length
}
