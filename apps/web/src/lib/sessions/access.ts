import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessions, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import type { Db } from "@/lib/sessions/tx"

// EXP-1291: the session tree's ONE visibility rule, server-side. `shared`
// (stamped at creation: the device is shared with the session's team; a
// child inherits its root's) → every team member reads, posts, answers and
// stops it; unshared → only its starter (`user_id`). The hosting device
// owner (`host_user_id`, a teammate's start on a shared box) is the third
// principal: it runs there. Mirrors the shapes' `shared OR user_id = me`.

const notFound = () =>
  new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

/** The pure rule (no membership): may this user see the row at all? */
export function sessionVisibleTo(
  session: Pick<Session, `shared` | `userId` | `hostUserId`>,
  userId: string
): boolean {
  return session.shared || session.userId === userId || session.hostUserId === userId
}

/** The session, if the caller may see it: a member of its team AND (shared
 *  OR the starter/host). NOT_FOUND otherwise (no existence oracle). */
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
 *  → any member; else the starter or the host). Re-checks membership so a
 *  caller holding a stale row cannot act after removal. */
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

/** The device procedures (brief/claim/report/reply/consume): only the
 *  account the session's device runs under — the host for a teammate's
 *  start on a shared box, else the starter. */
export function assertDeviceHost(userId: string, session: Session): void {
  if ((session.hostUserId ?? session.userId) !== userId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the device running this session can report for it`,
    })
  }
}

/** The account that runs the session's device. */
export function sessionDeviceOwner(session: Pick<Session, `userId` | `hostUserId`>): string {
  return session.hostUserId ?? session.userId
}
