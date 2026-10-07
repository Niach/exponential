import { createHash } from "node:crypto"
import { auth } from "@/lib/auth"
import { db } from "@/db/connection"
import {
  deriveClientPlatform,
  touchUserClientPlatform,
} from "@/lib/client-platforms"
import { authDbFailureCount } from "@/lib/auth/db-failure-signal"
import {
  apiKeySessionMetadata,
  type ApiKeyScope,
} from "@/lib/auth/api-key-kind"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

type Session = Awaited<ReturnType<typeof auth.api.getSession>>

// FEED-76: what a credential resolved to — the session AND, when the request
// rode a SCOPED `expu_` key, the team/board scope that key was minted with.
export interface ResolvedCredential {
  session: Session
  /** Non-null only behind a scoped personal key (lib/auth/api-key-kind.ts). */
  keyScope: ApiKeyScope | null
}

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
// - `retain` drops null resolutions, so a dead/revoked token is never cached
//   and the shape-route 401 path semantics stay unchanged. A getSession
//   FAILURE is a rejected promise (SessionResolveError, below) — TtlPromiseCache
//   evicts rejected entries on settle, so a transient DB blip is never cached
//   either; both cost exactly one lookup per call.
// - The cached credential (Session + key scope) is SHARED across callers —
//   treat it as read-only (all current callers do). FEED-76: the scoped-key
//   lookup (one PK probe on `apikeys`, only for `expu_` credentials) rides
//   the same entry, so a key costs one getSession + one probe per TTL.
// - Revocation bound: revokePersonalApiKey / account deletion clear the cache
//   in-process; anything else (e.g. a mobile bearer sign-out's server-side
//   session row death) rides the 30s TTL — well inside the 5-min cookieCache
//   precedent web sessions already live with.
const SESSION_CACHE_TTL_MS = 30_000
const sessionCache = new TtlPromiseCache<ResolvedCredential>({
  ttlMs: SESSION_CACHE_TTL_MS,
  maxEntries: 2_000,
  retain: (resolved) => Boolean(resolved.session?.user),
})

export function invalidateSessionCache(): void {
  sessionCache.clear()
}

// The auth chokepoint for the general API surface (tRPC, shape proxies,
// attachment/image routes). Resolves a request to a session, accepting:
//   - the session cookie (web) and `Authorization: Bearer <sessionToken>` (mobile)
//     via the bearer plugin, and
//   - `Authorization: Bearer expu_...` UNSCOPED personal api keys (apiKey plugin).
// Human MCP clients' OAuth2 access tokens are deliberately NOT accepted here:
// those tokens are consent-scoped to selected teams/boards, and only
// the MCP tool layer enforces that scope — so /api/mcp is the only endpoint
// that resolves them (see lib/mcp/scope.ts). FEED-76: a SCOPED personal key
// is the same kind of credential and reads as unauthenticated here too; only
// /api/mcp resolves it (resolveMcpCredential) and confines it to its scope.
export async function resolveSession(request: Request): Promise<Session> {
  const resolved = await resolveCredentialCore(request)
  if (resolved.keyScope) return null
  touchClientPlatform(request, resolved.session)
  return resolved.session
}

// /api/mcp's chokepoint: the session PLUS the key scope it must be confined
// to (`keyScope` null = the user's full membership).
export async function resolveMcpCredential(
  request: Request
): Promise<ResolvedCredential> {
  const resolved = await resolveCredentialCore(request)
  touchClientPlatform(request, resolved.session)
  return resolved
}

// EXP-759: the ONE place every authenticated API request passes (tRPC, all
// shape proxies, attachments, MCP) — record which client made it. Runs on
// cache hits too (the request is always in hand); throttled + fire-and-forget
// inside, never awaited, never throws.
function touchClientPlatform(request: Request, session: Session): void {
  const userId = session?.user?.id
  if (!userId) return
  const client = deriveClientPlatform(request)
  if (client) touchUserClientPlatform(db, { userId, ...client })
}

// Matches the apiKey plugin's `customAPIKeyGetter` (lib/auth/index.ts): the
// only two header forms that can resolve to an `expu_` key row. A mobile
// session bearer never pays the scope probe.
function carriesApiKey(authorization: string | null, apiKey: string | null) {
  return Boolean(apiKey) || /^Bearer\s+expu_/i.test(authorization ?? ``)
}

async function resolveCredentialCore(
  request: Request
): Promise<ResolvedCredential> {
  const authorization = request.headers.get(`authorization`)
  const apiKey = request.headers.get(`x-api-key`)
  if (!authorization && !apiKey) {
    // Cookie-only: never an api key (the plugin reads only the two token
    // headers), so no scope probe and no caching — cookieCache covers it.
    return { session: await getSessionBearerOnly(request), keyScope: null }
  }
  const cacheKey = createHash(`sha256`)
    .update(`${authorization ?? ``}\0${apiKey ?? ``}`)
    .digest(`base64url`)
  return sessionCache.get(cacheKey, async () => {
    const session = await getSessionBearerOnly(request)
    if (!session?.user || !carriesApiKey(authorization, apiKey)) {
      return { session, keyScope: null }
    }
    let metadata
    try {
      metadata = await apiKeySessionMetadata(db, session)
    } catch (err) {
      throw new SessionResolveError(`Session lookup failed (api key scope)`, {
        cause: err,
      })
    }
    return { session, keyScope: metadata?.scope ?? null }
  })
}

// Thrown when the session lookup itself FAILED (DB down, auth backend blip) —
// as opposed to resolving cleanly to "no session", which stays a null return.
// Callers must keep the two apart: swallowing a failure as null makes a
// transient outage look like a bad credential, and a native client answered
// 401 stops retrying and enters its unauthorized backoff (EXP-264).
export class SessionResolveError extends Error {}

async function getSessionBearerOnly(request: Request): Promise<Session> {
  let session: Session
  const failuresBefore = authDbFailureCount()
  try {
    session = await auth.api.getSession({ headers: bearerOnlyHeaders(request) })
  } catch (err) {
    throw new SessionResolveError(`Session lookup failed`, { cause: err })
  }
  if (session?.user) return session
  // REV2-20: null is ambiguous here — Better Auth's customSession plugin
  // catches a FAILED core lookup and answers null, so "logged out" and "the
  // session store is down" arrive identically. An auth database failure
  // recorded while this lookup was in flight breaks the tie (see
  // db-failure-signal.ts): report it as a failure so callers answer 503
  // instead of 401 / the anonymous sentinel shape.
  if (authDbFailureCount() !== failuresBefore) {
    throw new SessionResolveError(`Session lookup failed (auth database error)`)
  }
  return null
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
