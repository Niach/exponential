import { auth } from "@/lib/auth"

type Session = Awaited<ReturnType<typeof auth.api.getSession>>

// The auth chokepoint for the general API surface (tRPC, shape proxies,
// attachment/image routes). Resolves a request to a session, accepting:
//   - the session cookie (web) and `Authorization: Bearer <sessionToken>` (mobile)
//     via the bearer plugin, and
//   - `Authorization: Bearer expu_...` personal api keys (apiKey plugin).
// Human MCP clients' OAuth2 access tokens are deliberately NOT accepted here:
// those tokens are consent-scoped to selected workspaces/projects, and only
// the MCP tool layer enforces that scope — so /api/mcp is the only endpoint
// that resolves them (see lib/mcp/scope.ts).
export async function resolveSession(request: Request): Promise<Session> {
  const session = await auth.api
    .getSession({ headers: request.headers })
    .catch(() => null)
  return session?.user ? session : null
}

export async function resolveSessionUserId(
  request: Request
): Promise<string | null> {
  const session = await resolveSession(request)
  return session?.user?.id ?? null
}
