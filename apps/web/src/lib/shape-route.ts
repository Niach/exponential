import { resolveSession } from "@/lib/auth/resolve-bearer"
import { checkClientVersion } from "@/lib/client-version"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"

// True when the request carries explicit token credentials — an
// `Authorization` header (session bearer, `expu_` api key, or MCP OAuth
// token) or an `x-api-key` header. Session COOKIES are deliberately not
// counted: the web collection layer has no 401 recovery, so an expired
// cookie keeps the anonymous fallback and the router auth guard
// re-authenticates on next navigation.
function hasTokenCredentials(request: Request): boolean {
  return Boolean(
    request.headers.get(`authorization`) || request.headers.get(`x-api-key`)
  )
}

interface ShapeRouteHandlerOptions {
  // userId is null when the request is anonymous. Every shape is member-only
  // (EXP-180): the proxy is expected to return the impossible-match sentinel
  // in that case — anonymous callers sync nothing.
  getWhere?: (userId: string | null) => Promise<string | null | undefined>
  table: string
  // If true, anonymous requests are rejected with 401 even when getWhere can
  // produce a where clause. Use for tables that only make sense for an
  // authenticated user (notifications, push subscriptions, etc).
  requireAuth?: boolean
  // Optional column allowlist forwarded as Electric's `columns` param. Use to
  // keep sensitive columns out of a synced shape entirely (must include the
  // primary key). Set server-side, so clients cannot widen it. May be a
  // function of the resolved userId so anonymous requests can pin a TIGHTER
  // list than members (deterministic per caller class — columns are part of
  // Electric's shape identity, same stability rule as the where clause).
  columns?: string[] | ((userId: string | null) => string[] | undefined)
}

export function createShapeRouteHandler({
  getWhere,
  table,
  requireAuth = false,
  columns,
}: ShapeRouteHandlerOptions) {
  return async ({ request }: { request: Request }) => {
    // Outdated native clients get 426 before anything else — sync engines
    // treat it as a hard stop and the app shows its blocking update screen.
    const upgradeRequired = checkClientVersion(request)
    if (upgradeRequired) return upgradeRequired

    // Auth accepts the session cookie (web), `Authorization: Bearer
    // <sessionToken>` (iOS, Android), and personal `expu_` api keys via the
    // resolveSession chokepoint. Human MCP clients' OAuth2 access tokens are
    // deliberately NOT accepted — those are consent-scoped and only /api/mcp
    // resolves them (see lib/auth/resolve-bearer.ts) — so a presented MCP
    // token fails to resolve and 401s below. Null when anonymous.
    const session = await resolveSession(request)

    // A request that PRESENTED token credentials but failed to resolve a
    // session (revoked api key, expired mobile session token, dead MCP token)
    // must NOT fall back to the anonymous where clause: that silently swaps
    // the shape identity with HTTP 200 and 409-loops native sync engines.
    // Explicit 401 so the client re-authenticates. Requests with no token
    // credentials at all keep the anonymous fallback — the impossible-match
    // sentinel, an empty shape rather than an error.
    if (!session && (requireAuth || hasTokenCredentials(request))) {
      return new Response(`Unauthorized`, { status: 401 })
    }

    const originUrl = prepareElectricUrl(request.url)
    originUrl.searchParams.set(`table`, table)

    const userId = session?.user?.id ?? null
    const resolvedColumns =
      typeof columns === `function` ? columns(userId) : columns
    if (resolvedColumns) {
      originUrl.searchParams.set(`columns`, resolvedColumns.join(`,`))
    }

    const where = await getWhere?.(userId)

    if (where) {
      originUrl.searchParams.set(`where`, where)
    }

    return proxyElectricRequest(originUrl, request.signal)
  }
}
