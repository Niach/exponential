// A tRPC context for a SERVER-initiated call on a user's behalf — no HTTP
// request, no Better Auth session. The routers only read `ctx.session.user.id`
// and `ctx.db`; the rest keeps the Context type honest. Two callers: the
// import pipeline (`lib/import/apply-db.ts`, acting as the importer) and
// other server-side calls acting as a user.
import { db } from "@/db/connection"
import type { Context } from "@/lib/trpc"

export interface ActorUser {
  id: string
  email: string
  name: string
  image: string | null
  emailVerified: boolean
  createdAt: Date
  updatedAt: Date
}

export function buildServerActorContext(
  user: ActorUser,
  tag: string,
  opts: { viaMcp?: boolean } = {}
): Context {
  const now = new Date()
  return {
    db,
    request: new Request(`http://${tag}.local/`),
    ...(opts.viaMcp ? { viaMcp: true as const } : {}),
    session: {
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        image: user.image,
        emailVerified: user.emailVerified,
        createdAt: user.createdAt,
        updatedAt: user.updatedAt,
      },
      session: {
        id: tag,
        userId: user.id,
        token: tag,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000),
        createdAt: now,
        updatedAt: now,
        ipAddress: null,
        userAgent: tag,
      },
    },
  } as unknown as Context
}
