import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessions, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import type { Db } from "@/lib/sessions/tx"

export { assertDeviceHost, isSessionPrincipal, sessionDeviceOwner } from "@/lib/sessions/host"

// EXP-1291: the session tree's ONE visibility rule, server-side. `shared`
// (stamped at creation: the device is shared with the session's team; a
// child inherits its root's) → every team member reads, posts, answers and
// stops it; unshared → only its starter (`user_id`). Mirrors the shapes'
// `shared OR user_id = me`. NEW-1: the account hosting a teammate's start
// is derived from the devices rows (`lib/sessions/host.ts`); a hosted
// start sits on a shared device, so the host reads it like every member.

const notFound = () =>
  new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

/** The pure rule (no membership): may this user see the row at all? */
export function sessionVisibleTo(
  session: Pick<Session, `shared` | `userId`>,
  userId: string
): boolean {
  return session.shared || session.userId === userId
}

/** The session, if the caller may see it: a member of its team AND (shared
 *  OR the starter). NOT_FOUND otherwise (no existence oracle). */
export async function loadSessionForUser(
  userId: string,
  sessionId: string,
  database: Db = db
): Promise<Session> {
  const [session] = await database
    .select()
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!session || !sessionVisibleTo(session, userId)) throw notFound()
  const member = await getTeamMember(userId, session.teamId)
  if (!member) throw notFound()
  return session
}

/** Posting, answering, stopping, ending: the same rule as reading (shared
 *  → any member; else the starter). Re-checks membership so a caller
 *  holding a stale row cannot act after removal. */
export async function assertCanSteer(userId: string, session: Session): Promise<void> {
  if (!sessionVisibleTo(session, userId)) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the session's starter can act on a private session`,
    })
  }
  const member = await getTeamMember(userId, session.teamId)
  if (!member) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member of this team` })
  }
}
