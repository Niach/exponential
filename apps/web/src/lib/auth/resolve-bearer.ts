import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { auth } from "@/lib/auth"

type Session = Awaited<ReturnType<typeof auth.api.getSession>>

// Resolve a human MCP client's OAuth2 access token to its user id.
// `getMcpSession` is a plain DB lookup on `oauth_access_tokens.access_token`
// and returns the row (with `userId`).
export async function resolveMcpUserId(request: Request): Promise<string | null> {
  const mcpSession = await auth.api
    .getMcpSession({ request, headers: request.headers, asResponse: false })
    .catch(() => null)
  return mcpSession?.userId ?? null
}

// The single auth chokepoint. Resolves a request to a session, accepting:
//   - the session cookie (web) and `Authorization: Bearer <sessionToken>` (mobile)
//     via the bearer plugin,
//   - `Authorization: Bearer expu_...` personal api keys (apiKey plugin), and
//   - a human MCP client's refreshable OAuth2 access token via `getMcpSession`.
// The cheap session/api-key path runs first so hot web/mobile traffic pays no
// extra DB lookup; only requests that aren't a session/api-key fall through to
// the MCP token lookup. Returns a session-shaped object so every authedProcedure
// and shape proxy can read `session.user.id` uniformly.
export async function resolveSession(request: Request): Promise<Session> {
  const session = await auth.api
    .getSession({ headers: request.headers })
    .catch(() => null)
  if (session?.user) return session

  const mcpUserId = await resolveMcpUserId(request)
  if (!mcpUserId) return null

  const [user] = await db
    .select()
    .from(users)
    .where(eq(users.id, mcpUserId))
    .limit(1)
  if (!user) return null

  const now = new Date()
  return {
    user,
    session: {
      id: `mcp-${user.id}`,
      token: ``,
      userId: user.id,
      expiresAt: new Date(now.getTime() + 60 * 60 * 1000),
      createdAt: now,
      updatedAt: now,
      ipAddress: null,
      userAgent: null,
    },
  } as unknown as Session
}

export async function resolveSessionUserId(
  request: Request
): Promise<string | null> {
  const session = await resolveSession(request)
  return session?.user?.id ?? null
}
