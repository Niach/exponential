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
    .getSession({ headers: bearerOnlyHeaders(request) })
    .catch(() => null)
  return session?.user ? session : null
}

// A token-authenticated request (mobile session bearer or `expu_` api key in
// the `Authorization` / `x-api-key` header) MUST resolve its identity from the
// token alone. Better Auth sets a signed `__Secure-better-auth.session_data`
// cookie on every authenticated response (a 5-minute session-cache snapshot),
// and getSession trusts that cookie OVER the bearer. A client that shares one
// cookie jar across accounts on the same host (iOS ShapeClient did) therefore
// replays a PREVIOUS user's session_data cookie alongside the new user's bearer
// — and getSession resolves the request as the previous user, so shapes sync
// the old account's data under the new token (a cross-account leak that
// survives any client-side rebind). Strip the Cookie header when a token
// credential is present so the stale cache can't be hit; the bearer/apiKey
// plugins re-derive the session from their own header. Cookie-only (web)
// requests — which carry neither header — are untouched. (Matches
// shape-route.ts's `hasTokenCredentials`: both header forms are token creds.)
function bearerOnlyHeaders(request: Request): Headers {
  const hasToken =
    request.headers.get(`authorization`) || request.headers.get(`x-api-key`)
  if (!hasToken) return request.headers
  const headers = new Headers(request.headers)
  headers.delete(`cookie`)
  return headers
}

export async function resolveSessionUserId(
  request: Request
): Promise<string | null> {
  const session = await resolveSession(request)
  return session?.user?.id ?? null
}
