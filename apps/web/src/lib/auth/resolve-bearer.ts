import { createHash } from "node:crypto"
import { auth } from "@/lib/auth"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

type Session = Awaited<ReturnType<typeof auth.api.getSession>>

// REV2-7: short-TTL cache for TOKEN-credentialed sessions only. The cookie
// strip below (bearerOnlyHeaders) deliberately bypasses Better Auth's 5-min
// cookieCache for bearer/`expu_` clients, so before this cache every one of a
// native client's 14 Electric shape long-poll renewals per ~60s cycle was a
// real session/apikey DB lookup (plus the customSession plugin's onboarding/
// dismissal resolver queries and the apiKey plugin's lastRequest write).
// Cookie-only (web) requests are NEVER cached here — cookieCache covers them.
//
// - Keys are sha256 hashes of the credential headers, so no raw bearer
//   secrets are retained in long-lived memory. The cookie cannot influence
//   the result (it is stripped on every miss), so token-only keying is sound.
// - `retain` drops null resolutions: dead/revoked tokens and transient
//   getSession errors (normalized to null below) are never cached, keeping
//   the shape-route 401 path semantics unchanged.
// - The cached Session object is SHARED across callers — treat it as
//   read-only (all current callers do).
// - Revocation bound: revokePersonalApiKey / account deletion clear the cache
//   in-process; anything else (e.g. a mobile bearer sign-out's server-side
//   session row death) rides the 30s TTL — well inside the 5-min cookieCache
//   precedent web sessions already live with.
const SESSION_CACHE_TTL_MS = 30_000
const sessionCache = new TtlPromiseCache<Session>({
  ttlMs: SESSION_CACHE_TTL_MS,
  maxEntries: 2_000,
  retain: (session) => Boolean(session?.user),
})

export function invalidateSessionCache(): void {
  sessionCache.clear()
}

// The auth chokepoint for the general API surface (tRPC, shape proxies,
// attachment/image routes). Resolves a request to a session, accepting:
//   - the session cookie (web) and `Authorization: Bearer <sessionToken>` (mobile)
//     via the bearer plugin, and
//   - `Authorization: Bearer expu_...` personal api keys (apiKey plugin).
// Human MCP clients' OAuth2 access tokens are deliberately NOT accepted here:
// those tokens are consent-scoped to selected teams/boards, and only
// the MCP tool layer enforces that scope — so /api/mcp is the only endpoint
// that resolves them (see lib/mcp/scope.ts).
export async function resolveSession(request: Request): Promise<Session> {
  const authorization = request.headers.get(`authorization`)
  const apiKey = request.headers.get(`x-api-key`)
  if (!authorization && !apiKey) {
    return getSessionBearerOnly(request)
  }
  const cacheKey = createHash(`sha256`)
    .update(`${authorization ?? ``}\0${apiKey ?? ``}`)
    .digest(`base64url`)
  return sessionCache.get(cacheKey, () => getSessionBearerOnly(request))
}

async function getSessionBearerOnly(request: Request): Promise<Session> {
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
